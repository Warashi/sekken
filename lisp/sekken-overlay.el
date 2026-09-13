;;; sekken-overlay.el --- 入力中の語を置き換えて見せる overlay -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; バッファの中身はローマ字のまま、ポイント直前の語だけ overlay の
;; display で別の文字列に見せる。何を見せるかは呼ぶ側（sekken-live）が決める。
;; ポイントが語の末尾にあるときだけ出す。語の途中にポイントがあると
;; display 置換の中に入って動きが読めなくなる。

;;; Code:

(defface sekken-preedit
  '((t :inherit underline))
  "入力中の語の表示に使う face。"
  :group 'sekken)

(defvar-local sekken-overlay--overlay nil
  "このバッファの入力中の語を表示する overlay。")

(defun sekken-overlay-clear ()
  "overlay を消す。"
  (when sekken-overlay--overlay
    (delete-overlay sekken-overlay--overlay)
    (setq sekken-overlay--overlay nil)))

(defun sekken-overlay-show (start end display)
  "START から END の語を DISPLAY で見せる overlay を張り直す。"
  (unless sekken-overlay--overlay
    (setq sekken-overlay--overlay (make-overlay start end nil t nil)))
  (move-overlay sekken-overlay--overlay start end)
  (overlay-put sekken-overlay--overlay 'display display)
  (overlay-put sekken-overlay--overlay 'face 'sekken-preedit))

(provide 'sekken-overlay)
;;; sekken-overlay.el ends here
