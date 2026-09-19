;;; sekken-input-test.el --- sekken-input のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-input)
(require 'sekken-test)

;; 解析結果の 1 つだけを見る入口。本体は `sekken-input-analyze' を使うので、
;; テストの期待値を短く書くためにここで定義する。

(defun sekken-input-display (roman)
  "ROMAN の表示文字列。"
  (plist-get (sekken-input-analyze roman) :display))

(defun sekken-input-ready-p (roman)
  "ROMAN を先読みしてよいか。"
  (plist-get (sekken-input-analyze roman) :ready))

(defun sekken-input-converts-p (roman)
  "ROMAN に辞書を引く区間があるか。"
  (plist-get (sekken-input-analyze roman) :converts))

(defun sekken-input-commit (roman)
  "ROMAN を候補を使わずに置き換える文字列。"
  (plist-get (sekken-input-analyze roman) :commit))

(defun sekken-input-has-boundary-p (roman)
  "ROMAN が変換境界を含むか。"
  (and (cdr (sekken-input-segment roman)) t))

(ert-deftest sekken-input/1_度の解析から表示と先読みと送信を導く ()
  ;; 境界で終わる語は、表示に ▽ を残したまま、閉じた区間だけを送る。
  (let ((analysis (sekken-input-analyze "Neko;")))
    (should (equal (plist-get analysis :display) "▽ねこ▽"))
    (should (equal (plist-get analysis :pieces) [(:kind "convert" :text "ねこ")]))
    (should (plist-get analysis :converts))
    (should-not (plist-get analysis :ready)))
  (let ((analysis (sekken-input-analyze "kyouha'Emacs")))
    (should (plist-get analysis :ready))
    (should-not (plist-get analysis :converts))
    (should (equal (plist-get analysis :commit) "きょうはEmacs"))))

(ert-deftest sekken-input/確定の文字列は表示から境界の印だけを落とす ()
  (pcase-dolist (`(,roman ,display ,commit)
                 '(("Neko" "▽ねこ" "ねこ")
                   ("NekoGa" "▽ねこ▽が" "ねこが")
                   ("kyouHa" "きょう▽は" "きょうは")
                   ("Neko;" "▽ねこ▽" "ねこ")
                   ("/emacs" "▽/emacs" "emacs")
                   ("'Emacs" "▽'Emacs" "Emacs")
                   ("O>" "▽お>▽" "お")
                   ;; 母音を待つ子音は、見えているとおり綴りのまま確定する。
                   ("Nek" "▽ねk" "ねk")
                   ;; 境界で閉じた区間は母音を待たないので、表示も確定も「ねっ」。
                   ("Nek;" "▽ねっ▽" "ねっ")))
    (let ((analysis (sekken-input-analyze roman)))
      (should (equal (plist-get analysis :display) display))
      (should (equal (plist-get analysis :commit) commit)))))

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
  ;; 閉じた区間の末尾の子音は、もう母音を待たないので変換する。
  (should (equal (sekken-input-pieces "Nek;") [(:kind "convert" :text "ねっ")]))
  (should (equal (sekken-input-pieces "nek'") [(:kind "kana" :text "ねっ")]))
  ;; 境界だけなら何も残らない。
  (should (equal (sekken-input-pieces ";") []))
  (should (equal (sekken-input-pieces ">") []))
  (should (equal (sekken-input-commit "'git branch;") "git branch"))
  (should (equal (sekken-input-commit "neko'") "ねこ"))
  (should-not (sekken-input-converts-p "'emacs'"))
  ;; 表示は境界が開いていることを示したまま。
  (should (equal (sekken-input-display "Neko;") "▽ねこ▽")))

(ert-deftest sekken-input/変換境界の有無を判定する ()
  (should (sekken-input-has-boundary-p "Neko"))
  (should (sekken-input-has-boundary-p ";neko"))
  (should (sekken-input-has-boundary-p "semi;;;koron"))
  (should-not (sekken-input-has-boundary-p "neko"))
  (should-not (sekken-input-has-boundary-p "semi;;koron")))

(ert-deftest sekken-input/境界の後に読みがあれば先読みする ()
  ;; 境界で終わる語も確定はできるが、先読みはせず表示に境界を残す。
  (should (sekken-input-ready-p ";neko"))
  (should (sekken-input-ready-p "N"))
  (should-not (sekken-input-ready-p ";"))
  (should-not (sekken-input-ready-p "Neko;"))
  (should-not (sekken-input-ready-p ";;")))

(ert-deftest sekken-input/literal_の中の空白は綴りのまま送り_表示する ()
  (should (equal (sekken-input-pieces "Kyouha'git branch;wo")
                 [(:kind "convert" :text "きょうは")
                  (:kind "literal" :text "git branch")
                  (:kind "convert" :text "を")]))
  (should (equal (sekken-input-display "'git branch") "▽'git branch"))
  (should (equal (sekken-input-commit "'git branch") "git branch")))

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

(ert-deftest sekken-input/二重の記号はどの区間でもその文字_1_つになる ()
  (should (equal (sekken-input-pieces "'don''t") [(:kind "literal" :text "don't")]))
  (should (equal (sekken-input-pieces "ka''na") [(:kind "kana" :text "か'な")]))
  (should (equal (sekken-input-display "'don''t") "▽'don't"))
  (should-not (sekken-input-has-boundary-p "ka''na"))
  ;; `/' は abbrev の入口なので、和文の中の `/' は `//' で打つ。
  (should (equal (sekken-input-pieces "a//b") [(:kind "kana" :text "あ/b")]))
  (should (equal (sekken-input-commit "Neko//Ha") "ねこ/は"))
  (should-not (sekken-input-has-boundary-p "a//b"))
  ;; 綴りのままの区間の中でも閉じずにその文字になる。
  (should (equal (sekken-input-pieces "'a//b") [(:kind "literal" :text "a/b")]))
  (should (equal (sekken-input-pieces "'a;;b") [(:kind "literal" :text "a;b")]))
  (should (equal (sekken-input-pieces "/a//b") [(:kind "abbrev" :text "a/b")])))

(ert-deftest sekken-input/変換表のキーは境界の文字より長く一致する ()
  ;; `z/' は ・ のキーなので、`z' の後の `/' は abbrev の入口にならない。
  (should (equal (sekken-input-pieces "nekoz/") [(:kind "kana" :text "ねこ・")]))
  (should (equal (sekken-input-display "Nekoz/Ha") "▽ねこ・▽は"))
  (should-not (sekken-input-has-boundary-p "nekoz/"))
  ;; 2 文字目の `/' にはもう `z' が無いので境界。
  (should (equal (sekken-input-display "z//") "・▽/"))
  ;; 綴りのままの区間では表を引かない。
  (should (equal (sekken-input-pieces "'z//;ha")
                 [(:kind "literal" :text "z/") (:kind "convert" :text "は")]))
  ;; 大文字はキーの一部にならない。
  (should (equal (sekken-input-display "zH") "っ▽h")))

(ert-deftest sekken-input/境界の直後のアポストロフィはその区間を_literal_にする ()
  (should (equal (sekken-input-pieces "O>'emacs")
                 [(:kind "convert" :text "お" :prefix t) (:kind "literal" :text "emacs")]))
  (should (equal (sekken-input-pieces ";'emacs") [(:kind "literal" :text "emacs")])))

(ert-deftest sekken-input/辞書を引く区間が無ければエンジンに送らずに文字列にする ()
  (should (equal (sekken-input-commit "kyouha'Emacs") "きょうはEmacs"))
  (should (equal (sekken-input-commit "'don''t") "don't"))
  (should (equal (sekken-input-commit "neko") "ねこ")))

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
  ;; `>' で終わる語は送る区間が手前の語と違うので先読みする。
  (should (sekken-input-ready-p "O>")))

(defun sekken-input-test--piece (field)
  "共有 fixture の `KIND:TEXT' 形の FIELD を、期待する区間の plist にする。
TEXT は空白や `:' を含みうるので、最初の `:' だけで切る。"
  (let* ((colon (string-search ":" field))
         (kind (substring field 0 colon))
         (text (substring field (1+ colon))))
    (append (list :kind (car (split-string kind "\\+" t))
                  :text text)
            (and (string-search "+prefix" kind) '(:prefix t))
            (and (string-search "+suffix" kind) '(:suffix t)))))

(defun sekken-input-test--cases ()
  "共有 fixture の (ROMAN 表示 区間...) の並び。"
  (with-temp-buffer
    (insert-file-contents
     (expand-file-name "../share/input-cases.tsv"
                       (file-name-directory
                        (symbol-file 'sekken-input-segment 'defun))))
    (let (cases)
      (dolist (line (split-string (buffer-string) "\n" t))
        (unless (string-prefix-p "#" line)
          (push (split-string line "\t") cases)))
      (nreverse cases))))

(ert-deftest sekken-input/Rust_と共通の_fixture_と一致する ()
  "同じローマ字を Rust と Emacs が同じ区間に分けないと、先読みと確定がずれる。
表示は Emacs にしかないので、この表の 2 列目は Emacs だけが読む。"
  (let ((cases (sekken-input-test--cases)))
    (should (> (length cases) 30))
    (dolist (case cases)
      (let ((roman (nth 0 case)))
        (should (equal (cons roman (sekken-input-display roman))
                       (cons roman (nth 1 case))))
        (should (equal (cons roman (sekken-input-pieces roman))
                       (cons roman
                             (vconcat (mapcar #'sekken-input-test--piece
                                              (nthcdr 2 case))))))))))

(provide 'sekken-input-test)
;;; sekken-input-test.el ends here
