;;; sekken-live.el --- 打鍵に合わせて 1 位候補を見せるライブ変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 打鍵のたびに入力中の語の候補を先読みし、overlay に 1 位候補を見せる。
;; 候補がまだ無い間と、辞書を引く区間の無い語は、かな（と綴りのままの
;; 英字）を見せる。

;;; Code:

(require 'sekken-convert)
(require 'sekken-input)
(require 'sekken-overlay)

(defun sekken-live-display (roman)
  "入力中の ROMAN を overlay で見せる文字列。
候補を覚えていれば 1 位候補、無ければ境界を ▽ で示したかな表示。"
  (or (car (sekken-convert-cached roman))
      (sekken-input-display roman)))

(defun sekken-live--prefetch (roman)
  "辞書を引く ROMAN の候補を先読みし、届いたらこのバッファの overlay を張り直す。"
  (when (and (sekken-input-converts-p roman)
             (sekken-input-ready-p roman))
    (let ((buffer (current-buffer)))
      (sekken-convert-prefetch
       roman
       (lambda ()
         (when (buffer-live-p buffer)
           (with-current-buffer buffer
             (sekken-live-update))))))))

(defun sekken-live-update ()
  "ポイント直前の語に合わせて overlay を張り直し、候補が無ければ先読みする。"
  (let ((bounds (sekken-input-bounds)))
    (if (null bounds)
        (sekken-overlay-clear)
      (let* ((start (car bounds))
             (end (cdr bounds))
             (roman (buffer-substring-no-properties start end)))
        (sekken-overlay-show start end (sekken-live-display roman))
        (sekken-live--prefetch roman)))))

(provide 'sekken-live)
;;; sekken-live.el ends here
