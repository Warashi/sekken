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
  (should (equal (sekken-input-display "neko") "ねこ")))

(ert-deftest sekken-input/大文字の有無を判定する ()
  (should (sekken-input-has-upper-p "Neko"))
  (should-not (sekken-input-has-upper-p "neko")))

(provide 'sekken-input-test)
;;; sekken-input-test.el ends here
