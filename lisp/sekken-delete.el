;;; sekken-delete.el --- 入力中の語を見えている単位で消す後退削除 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; バッファにあるのはローマ字なので、1 文字ずつ消すと「猫g」のような途中の
;; 状態を経て、押した回数と見た目が対応しない。入力中の語ではかな 1 つ分
;; （`sekken-input-last-unit-length'）を消す。語の外とリージョンがあるときは、
;; `sekken-mode' が無ければそのキーに割り当たっていたコマンドを動かす。

;;; Code:

(require 'sekken-input)
(require 'sekken-word)

(defvar sekken-mode)

(defun sekken-delete--fallback-command ()
  "`sekken-mode' が無ければこのキーに割り当たっていたコマンド。
キーで呼ばれていなければ `delete-backward-char'。"
  (let* ((keys (this-command-keys-vector))
         (command (and (> (length keys) 0)
                       (let ((sekken-mode nil))
                         (key-binding keys t)))))
    (if (and (commandp command) (not (eq command #'sekken-delete-backward)))
        command
      #'delete-backward-char)))

(defun sekken-delete-backward (n)
  "ポイント直前の入力中の語から、末尾の N 単位を消す。
単位はかな 1 つ分で、語の打ち始めより前は消さない。語が無いか、リージョンが
あるか、N が正でなければ、このキーの元のコマンドを実行する。"
  (interactive "p")
  (let ((bounds (sekken-word-bounds)))
    (if (or (null bounds) (use-region-p) (<= n 0))
        (let ((fallback (sekken-delete--fallback-command)))
          (setq this-command fallback)
          (call-interactively fallback))
      (let ((start (car bounds))
            (end (cdr bounds)))
        (dotimes (_ n)
          (setq end (- end (sekken-input-last-unit-length
                            (buffer-substring-no-properties start end)))))
        (delete-region end (cdr bounds))))))

;; `delete-backward-char' と同じく、`delete-selection-mode' ではリージョンを消すだけにする。
(put 'sekken-delete-backward 'delete-selection 'supersede)

(provide 'sekken-delete)
;;; sekken-delete.el ends here
