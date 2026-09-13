;;; sekken-input-test.el --- sekken-input のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-input)

(ert-deftest sekken-input/ポイント直前のローマ字と記号を語として切り出す ()
  (with-temp-buffer
    (insert "hello WagahaihaNekodearu.")
    (should (equal (sekken-input-bounds) (cons 7 26)))
    (insert " ")
    (should (null (sekken-input-bounds)))))

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
  (should (equal (sekken-input-pieces "Kan") [(:kind "convert" :text "かん")]))
  (should (equal (sekken-input-pieces "Neko;")
                 [(:kind "convert" :text "ねこ") (:kind "convert" :text "")])))

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

(ert-deftest sekken-input/セミコロンを入力中の語に含める ()
  (with-temp-buffer
    (insert ";shokai;kougi")
    (should (equal (sekken-input-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-input/スラッシュと山括弧を入力中の語に含める ()
  (with-temp-buffer
    (insert "Kyou/emacs;O>Kai")
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
