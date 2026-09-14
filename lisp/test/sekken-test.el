;;; sekken-test.el --- テストで共有する補助 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'sekken-input)

(defun sekken-test-type (string)
  "STRING を打ったものとしてポイントに入れる。
`sekken-mode' が付ける編集の hook を、自己挿入のコマンドとして直接呼ぶ。"
  (let ((this-command #'self-insert-command)
        (beg (point)))
    (insert string)
    (sekken-input-after-change beg (point) 0)))

(provide 'sekken-test)
;;; sekken-test.el ends here
