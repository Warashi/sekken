;;; sekken-input-test.el --- sekken-input のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-input)
(require 'sekken-test)

(ert-deftest sekken-input/ポイント直前のローマ字と記号を語として切り出す ()
  (with-temp-buffer
    (sekken-test-type "hello WagahaihaNekodearu.")
    (should (equal (sekken-input-bounds) (cons 7 26)))
    (sekken-test-type " ")
    (should (null (sekken-input-bounds)))))

(ert-deftest sekken-input/打っていない文字は語にしない ()
  ;; org の PROPERTIES ドロワーのように、もとからバッファにある文字。
  (with-temp-buffer
    (insert ":END:")
    (should (null (sekken-input-bounds)))
    (sekken-test-type "ne")
    (should (equal (sekken-input-bounds) (cons 6 8)))))

(ert-deftest sekken-input/打ち始めより前に戻れば語は無い ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "ne")
    (goto-char 3)
    (should (null (sekken-input-bounds)))))

(ert-deftest sekken-input/自己挿入でない挿入では打ち始めにしない ()
  (with-temp-buffer
    (let ((this-command #'yank))
      (insert "neko")
      (sekken-input-after-change 1 5 0))
    (should (null (sekken-input-bounds)))))

(ert-deftest sekken-input/打ち始めより前を編集すれば忘れる ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "neko")
    (sekken-input-before-change 3 4)
    (delete-region 3 4)
    (should (null (sekken-input-bounds)))
    ;; 語の中の削除では忘れない。
    (sekken-test-type "ne")
    (sekken-input-before-change 8 9)
    (delete-region 8 9)
    (should (equal (sekken-input-bounds) (cons 7 8)))))

(ert-deftest sekken-input/忘れた後の打鍵は新しい打ち始めになる ()
  (with-temp-buffer
    (sekken-test-type "neko")
    (sekken-input-forget-origin)
    (should (null (sekken-input-bounds)))
    (sekken-test-type "ga")
    (should (equal (sekken-input-bounds) (cons 5 7)))))

(ert-deftest sekken-input/大文字境界を▽で示してかなにする ()
  (should (equal (sekken-input-display "WagahaihaNek") "▽わがはいは▽ねk"))
  (should (equal (sekken-input-display "kyouHa") "きょう▽は"))
  (should (equal (sekken-input-display "neko") "ねこ"))
  (should (equal (sekken-input-display ";shokai;kougi") "▽しょかい▽こうぎ"))
  (should (equal (sekken-input-display "semi;;koron") "せみ;ころん")))

(ert-deftest sekken-input/エンジンに送る種類付きのかな区間にする ()
  (should (equal (sekken-input-pieces "soreHaNekoDa")
                 [(:kind "kana" :text "それ")
                  (:kind "convert" :text "は")
                  (:kind "convert" :text "ねこ")
                  (:kind "convert" :text "だ")]))
  (should (equal (sekken-input-pieces ";shokai;kougi")
                 [(:kind "convert" :text "しょかい")
                  (:kind "convert" :text "こうぎ")]))
  (should (equal (sekken-input-pieces "neko") [(:kind "kana" :text "ねこ")]))
  (should (equal (sekken-input-pieces "") [])))

(ert-deftest sekken-input/最後の区間は末尾の子音を変換せずに送る ()
  (should (equal (sekken-input-pieces "KaK")
                 [(:kind "convert" :text "か") (:kind "convert" :text "k")]))
  (should (equal (sekken-input-pieces "Kak") [(:kind "convert" :text "かk")]))
  (should (equal (sekken-input-pieces "nek") [(:kind "kana" :text "ねk")]))
  (should (equal (sekken-input-pieces "Kan") [(:kind "convert" :text "かん")])))

(ert-deftest sekken-input/境界で終わる語は末尾の空の区間を落として送る ()
  ;; 末尾の `;' は前の語を閉じる意図しか持たないので、打たなかったものとして扱う。
  (should (equal (sekken-input-pieces "Neko;") (sekken-input-pieces "Neko")))
  (should (equal (sekken-input-pieces "'git branch;")
                 [(:kind "literal" :text "git branch")]))
  (should (equal (sekken-input-pieces "'emacs'") [(:kind "literal" :text "emacs")]))
  (should (equal (sekken-input-pieces "Neko/") [(:kind "convert" :text "ねこ")]))
  ;; 接頭辞の印は残し、`お>' の見出しで引けるようにする。
  (should (equal (sekken-input-pieces "O>") [(:kind "convert" :text "お" :prefix t)]))
  ;; 境界だけなら何も残らない。
  (should (equal (sekken-input-pieces ";") []))
  (should (equal (sekken-input-pieces ">") []))
  (should (equal (sekken-input-literal "'git branch;") "git branch"))
  (should (equal (sekken-input-literal "neko'") "ねこ"))
  (should-not (sekken-input-converts-p "'emacs'"))
  ;; 表示は境界が開いていることを示したまま。
  (should (equal (sekken-input-display "Neko;") "▽ねこ▽")))

(ert-deftest sekken-input/変換境界の有無を判定する ()
  (should (sekken-input-has-boundary-p "Neko"))
  (should (sekken-input-has-boundary-p ";neko"))
  (should (sekken-input-has-boundary-p "semi;;;koron"))
  (should-not (sekken-input-has-boundary-p "neko"))
  (should-not (sekken-input-has-boundary-p "semi;;koron")))

(ert-deftest sekken-input/境界の後に読みがあれば変換できる ()
  (should (sekken-input-ready-p ";neko"))
  (should (sekken-input-ready-p "N"))
  (should-not (sekken-input-ready-p ";"))
  (should-not (sekken-input-ready-p "Neko;"))
  (should-not (sekken-input-ready-p ";;")))

(ert-deftest sekken-input/数字と記号も入力中の語に含める ()
  ;; 1on1 のような数字混じりの語を 1 語として打てるよう、空白以外の
  ;; 打てる文字はすべて語の一部にする。
  (dolist (roman '("Kyou1on1" "'1on1" "(Neko)" "\"Neko\"" "Neko#1" "a@b"))
    (with-temp-buffer
      (sekken-test-type roman)
      (should (equal (sekken-input-bounds) (cons 1 (point-max)))))))

(ert-deftest sekken-input/空白と改行で語が終わる ()
  (dolist (delimiter '(" " "\n" "\t"))
    (with-temp-buffer
      (sekken-test-type "Neko")
      (sekken-test-type delimiter)
      (should (null (sekken-input-bounds)))
      (sekken-test-type "Ga")
      (should (equal (sekken-input-bounds) (cons 6 8))))))

(ert-deftest sekken-input/literal_の中の空白は語を切らない ()
  ;; 日本語の文に英語を数語挟んでも、文は改行まで 1 語のまま。
  (with-temp-buffer
    (sekken-test-type "Kyouha'git branch")
    (should (equal (sekken-input-bounds) (cons 1 (point-max))))
    (sekken-test-type ";wo;tsukau")
    (should (equal (sekken-input-bounds) (cons 1 (point-max)))))
  (with-temp-buffer
    (sekken-test-type "'git branch")
    (should (equal (sekken-input-bounds) (cons 1 (point-max)))))
  ;; `''' はリテラルのアポストロフィなので literal は続く。
  (with-temp-buffer
    (sekken-test-type "'don''t stop")
    (should (equal (sekken-input-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-input/literal_を閉じた後の空白は語を切る ()
  (dolist (roman '("'git;" "'git'" "'git/" "/git"))
    (with-temp-buffer
      (sekken-test-type roman)
      (sekken-test-type " ")
      (should (null (sekken-input-bounds)))
      (sekken-test-type "Ga")
      (should (equal (sekken-input-bounds)
                     (cons (+ (length roman) 2) (point-max)))))))

(ert-deftest sekken-input/literal_の中の空白は綴りのまま送り_表示する ()
  (should (equal (sekken-input-pieces "Kyouha'git branch;wo")
                 [(:kind "convert" :text "きょうは")
                  (:kind "literal" :text "git branch")
                  (:kind "convert" :text "を")]))
  (should (equal (sekken-input-display "'git branch") "▽'git branch"))
  (should (equal (sekken-input-literal "'git branch") "git branch")))

(ert-deftest sekken-input/セミコロンを入力中の語に含める ()
  (with-temp-buffer
    (sekken-test-type ";shokai;kougi")
    (should (equal (sekken-input-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-input/スラッシュと山括弧を入力中の語に含める ()
  (with-temp-buffer
    (sekken-test-type "Kyou/emacs;O>Kai")
    (should (equal (sekken-input-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-input/境界の直後の大文字やセミコロンは新しい区間を作らない ()
  (should (equal (sekken-input-pieces ";Kai") [(:kind "convert" :text "かい")]))
  (should (equal (sekken-input-pieces "Neko;Kai")
                 [(:kind "convert" :text "ねこ") (:kind "convert" :text "かい")]))
  (should (equal (sekken-input-pieces "Neko;;;kai")
                 [(:kind "convert" :text "ねこ;") (:kind "convert" :text "かい")]))
  (should (equal (sekken-input-pieces ";o>;kai") (sekken-input-pieces "O>Kai")))
  (should (equal (sekken-input-display ";o>;kai") "▽お>▽かい")))

(ert-deftest sekken-input/境界の直後のスラッシュはその区間を_abbrev_にする ()
  (should (equal (sekken-input-pieces "O>/emacs")
                 [(:kind "convert" :text "お" :prefix t) (:kind "abbrev" :text "emacs")]))
  (should (equal (sekken-input-pieces ";/emacs") [(:kind "abbrev" :text "emacs")])))

(ert-deftest sekken-input/スラッシュで開いた区間は綴りのまま_abbrev_で送る ()
  (should (equal (sekken-input-pieces "/emacs") [(:kind "abbrev" :text "emacs")]))
  ;; 中の大文字は境界にせず、末尾の子音も待たない。
  (should (equal (sekken-input-pieces "Kyou/GPL;ha")
                 [(:kind "convert" :text "きょう")
                  (:kind "abbrev" :text "GPL")
                  (:kind "convert" :text "は")]))
  (should (equal (sekken-input-pieces "/emacs/Ha")
                 [(:kind "abbrev" :text "emacs") (:kind "convert" :text "は")]))
  (should (equal (sekken-input-display "/emacs;ha") "▽/emacs▽は"))
  (should (sekken-input-has-boundary-p "/emacs"))
  (should (sekken-input-ready-p "/emacs"))
  (should-not (sekken-input-ready-p "/")))

(ert-deftest sekken-input/アポストロフィで開いた区間は綴りのまま_literal_で送る ()
  (should (equal (sekken-input-pieces "'emacs") [(:kind "literal" :text "emacs")]))
  ;; 中の大文字は境界にせず、末尾の子音も待たない。
  (should (equal (sekken-input-pieces "Kyouha'Emacs;wo")
                 [(:kind "convert" :text "きょうは")
                  (:kind "literal" :text "Emacs")
                  (:kind "convert" :text "を")]))
  ;; `;' `/' `'' のどれでも閉じ、続きは変換区間になる。
  (should (equal (sekken-input-pieces "'emacs'Ha")
                 [(:kind "literal" :text "emacs") (:kind "convert" :text "は")]))
  (should (equal (sekken-input-pieces "'emacs/Ha")
                 [(:kind "literal" :text "emacs") (:kind "convert" :text "は")]))
  (should (equal (sekken-input-pieces "/emacs'Ha")
                 [(:kind "abbrev" :text "emacs") (:kind "convert" :text "は")]))
  (should (equal (sekken-input-display "'emacs;ha") "▽'emacs▽は"))
  (should (sekken-input-has-boundary-p "'emacs"))
  (should (sekken-input-ready-p "'emacs"))
  (should-not (sekken-input-ready-p "'")))

(ert-deftest sekken-input/二重アポストロフィはリテラルのアポストロフィになる ()
  (should (equal (sekken-input-pieces "'don''t") [(:kind "literal" :text "don't")]))
  (should (equal (sekken-input-pieces "ka''na") [(:kind "kana" :text "か'な")]))
  (should (equal (sekken-input-display "'don''t") "▽'don't"))
  (should-not (sekken-input-has-boundary-p "ka''na")))

(ert-deftest sekken-input/境界の直後のアポストロフィはその区間を_literal_にする ()
  (should (equal (sekken-input-pieces "O>'emacs")
                 [(:kind "convert" :text "お" :prefix t) (:kind "literal" :text "emacs")]))
  (should (equal (sekken-input-pieces ";'emacs") [(:kind "literal" :text "emacs")])))

(ert-deftest sekken-input/辞書を引く区間が無ければエンジンに送らずに文字列にする ()
  (should (equal (sekken-input-literal "kyouha'Emacs") "きょうはEmacs"))
  (should (equal (sekken-input-literal "'don''t") "don't"))
  (should (equal (sekken-input-literal "neko") "ねこ")))

(ert-deftest sekken-input/変換する区間の有無を判定する ()
  (should (sekken-input-converts-p "Neko"))
  (should (sekken-input-converts-p "/emacs"))
  (should (sekken-input-converts-p "'emacs;ha"))
  (should-not (sekken-input-converts-p "'emacs"))
  (should-not (sekken-input-converts-p "neko'emacs"))
  (should-not (sekken-input-converts-p "neko")))

(ert-deftest sekken-input/山括弧は前後の区間に接頭辞と接尾辞の印を付ける ()
  (should (equal (sekken-input-pieces "O>Kai")
                 [(:kind "convert" :text "お" :prefix t)
                  (:kind "convert" :text "かい" :suffix t)]))
  (should (equal (sekken-input-pieces "Toukyou>kaiha")
                 [(:kind "convert" :text "とうきょう" :prefix t)
                  (:kind "convert" :text "かいは" :suffix t)]))
  (should (equal (sekken-input-pieces "O>Kai>Sha")
                 [(:kind "convert" :text "お" :prefix t)
                  (:kind "convert" :text "かい" :prefix t :suffix t)
                  (:kind "convert" :text "しゃ" :suffix t)]))
  ;; 先頭の小文字には印を付けず、接尾辞の区間だけを作る。
  (should (equal (sekken-input-pieces "o>kai")
                 [(:kind "kana" :text "お") (:kind "convert" :text "かい" :suffix t)]))
  (should (equal (sekken-input-display "O>Kai") "▽お>▽かい"))
  (should (sekken-input-has-boundary-p "o>kai"))
  (should-not (sekken-input-ready-p "O>")))

(provide 'sekken-input-test)
;;; sekken-input-test.el ends here
