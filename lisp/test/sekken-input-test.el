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

(provide 'sekken-input-test)
;;; sekken-input-test.el ends here
