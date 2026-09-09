;;; sekken-overlay-test.el --- sekken-overlay のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-overlay)

(ert-deftest sekken-overlay/入力中の語にかな表示の_overlay_を張る ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-overlay-update)
    (should sekken-overlay--overlay)
    (should (equal (overlay-get sekken-overlay--overlay 'display) "▽ねこ"))
    (should (= (overlay-start sekken-overlay--overlay) 1))
    (should (= (overlay-end sekken-overlay--overlay) 5))))

(ert-deftest sekken-overlay/語が無くなれば_overlay_を消す ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-overlay-update)
    (insert " ")
    (sekken-overlay-update)
    (should (null sekken-overlay--overlay))))

(provide 'sekken-overlay-test)
;;; sekken-overlay-test.el ends here
