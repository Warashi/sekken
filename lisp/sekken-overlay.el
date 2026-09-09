;;; sekken-overlay.el --- 入力中の語をかなで表示する overlay -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; バッファの中身はローマ字のまま、ポイント直前の語だけ overlay の
;; display でかなに見せる。ポイントが語の末尾にあるときだけ出す。
;; 語の途中にポイントがあると display 置換の中に入って動きが読めなくなる。

;;; Code:

(require 'sekken-input)

(defface sekken-preedit
  '((t :inherit underline))
  "入力中の語のかな表示に使う face。"
  :group 'sekken)

(defvar-local sekken-overlay--overlay nil
  "このバッファの入力中の語を表示する overlay。")

(defun sekken-overlay-clear ()
  "overlay を消す。"
  (when sekken-overlay--overlay
    (delete-overlay sekken-overlay--overlay)
    (setq sekken-overlay--overlay nil)))

(defun sekken-overlay-update ()
  "ポイント直前の語に合わせて overlay を張り直す。"
  (let ((bounds (sekken-input-bounds)))
    (if (null bounds)
        (sekken-overlay-clear)
      (let* ((start (car bounds))
             (end (cdr bounds))
             (roman (buffer-substring-no-properties start end))
             (display (sekken-input-display roman)))
        (unless sekken-overlay--overlay
          (setq sekken-overlay--overlay (make-overlay start end nil t nil)))
        (move-overlay sekken-overlay--overlay start end)
        (overlay-put sekken-overlay--overlay 'display display)
        (overlay-put sekken-overlay--overlay 'face 'sekken-preedit)))))

(provide 'sekken-overlay)
;;; sekken-overlay.el ends here
