;;; sekken-overlay-test.el --- sekken-overlay のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-overlay)

(ert-deftest sekken-overlay/語の範囲に表示の_overlay_を張る ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-overlay-show 1 5 "▽ねこ")
    (should sekken-overlay--overlay)
    (should (equal (overlay-get sekken-overlay--overlay 'display) "▽ねこ"))
    (should (= (overlay-start sekken-overlay--overlay) 1))
    (should (= (overlay-end sekken-overlay--overlay) 5))))

(ert-deftest sekken-overlay/張り直しは同じ_overlay_を動かす ()
  (with-temp-buffer
    (insert "Nekoga")
    (sekken-overlay-show 1 5 "▽ねこ")
    (let ((overlay sekken-overlay--overlay))
      (sekken-overlay-show 1 7 "猫が")
      (should (eq overlay sekken-overlay--overlay))
      (should (equal (overlay-get overlay 'display) "猫が"))
      (should (= (overlay-end overlay) 7)))))

(ert-deftest sekken-overlay/消せば_overlay_は無くなる ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-overlay-show 1 5 "▽ねこ")
    (sekken-overlay-clear)
    (should (null sekken-overlay--overlay))
    (should (null (overlays-in (point-min) (point-max))))))

(provide 'sekken-overlay-test)
;;; sekken-overlay-test.el ends here
