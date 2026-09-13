;;; sekken-live-test.el --- sekken-live のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-live)

(defmacro sekken-live-test--with-prefetch (requests &rest body)
  "先読みを、頼まれた (入力 . 知らせる関数) を REQUESTS に溜めるだけにして BODY を実行する。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-convert-prefetch)
              (lambda (roman notify) (push (cons roman notify) ,requests))))
     (setq sekken-convert--cache nil)
     ,@body))

(defun sekken-live-test--display ()
  "今の overlay の表示文字列。"
  (overlay-get sekken-overlay--overlay 'display))

(ert-deftest sekken-live/候補が無い間はかな表示で先読みを頼む ()
  (with-temp-buffer
    (insert "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (should (equal (sekken-live-test--display) "▽ねこ"))
        (should (equal (car (car requests)) "Neko"))))))

(ert-deftest sekken-live/候補が届けば_1_位候補を見せる ()
  (with-temp-buffer
    (insert "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (setq sekken-convert--cache '("Neko" "猫" "ねこ"))
        (funcall (cdr (car requests)))
        (should (equal (sekken-live-test--display) "猫"))))))

(ert-deftest sekken-live/届いた候補が今の語と違えばかな表示のまま先読みし直す ()
  (with-temp-buffer
    (insert "Ne")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (insert "ko")
        (setq sekken-convert--cache '("Ne" "ね"))
        (funcall (cdr (car requests)))
        (should (equal (sekken-live-test--display) "▽ねこ"))
        (should (equal (car (car requests)) "Neko"))))))

(ert-deftest sekken-live/辞書を引く区間の無い語は先読みせずかなを見せる ()
  (dolist (case '(("kyouha" . "きょうは")
                  ("kyouha'Emacs" . "きょうは▽'Emacs")
                  ("Neko;" . "▽ねこ▽")))
    (with-temp-buffer
      (insert (car case))
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (sekken-live-update)
          (should (equal (sekken-live-test--display) (cdr case)))
          (should-not requests))))))

(ert-deftest sekken-live/語が無くなれば_overlay_を消す ()
  (with-temp-buffer
    (insert "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (insert " ")
        (sekken-live-update)
        (should (null sekken-overlay--overlay))))))

(ert-deftest sekken-live/バッファが消えた後の返事は何もしない ()
  (let ((buffer (generate-new-buffer " *sekken live*"))
        requests)
    (with-current-buffer buffer
      (insert "Neko")
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)))
    (kill-buffer buffer)
    (funcall (cdr (car requests)))))

(provide 'sekken-live-test)
;;; sekken-live-test.el ends here
