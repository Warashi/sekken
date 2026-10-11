;;; sekken-delete-test.el --- sekken-delete のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken)
(require 'sekken-delete)
(require 'sekken-test)

(ert-deftest sekken-delete/入力中の語ではかな_1_つ分を消す ()
  (with-temp-buffer
    (sekken-test-type "Nekoga")
    (sekken-delete-backward 1)
    (should (equal (buffer-string) "Neko"))
    (sekken-delete-backward 1)
    (should (equal (buffer-string) "Ne"))))

(ert-deftest sekken-delete/前置引数の数だけ消し語の打ち始めで止まる ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "Nekoga")
    (sekken-delete-backward 2)
    (should (equal (buffer-string) "abcNe"))
    (sekken-delete-backward 5)
    (should (equal (buffer-string) "abc"))))

(defun sekken-delete-test-fallback ()
  "元のキーのコマンドの代わり。"
  (interactive)
  (insert "!"))

(ert-deftest sekken-delete/語が無ければ元のキーのコマンドを実行する ()
  (with-temp-buffer
    (insert "abc")
    (let ((this-command #'sekken-delete-backward))
      (cl-letf (((symbol-function 'sekken-delete--fallback-command)
                 (lambda () #'sekken-delete-test-fallback)))
        (sekken-delete-backward 1))
      (should (eq this-command #'sekken-delete-test-fallback)))
    (should (equal (buffer-string) "abc!"))))

(ert-deftest sekken-delete/キーで呼ばれていなければ元のコマンドは_1_文字の後退削除 ()
  (with-temp-buffer
    (insert "abc")
    (sekken-delete-backward 1)
    (should (equal (buffer-string) "ab"))))

(ert-deftest sekken-delete/リージョンがあれば語の中でも元のコマンドでリージョンを消す ()
  (with-temp-buffer
    (transient-mark-mode 1)
    (sekken-test-type "Nekoga")
    (set-mark 3)
    (activate-mark)
    (sekken-delete-backward 1)
    (should (equal (buffer-string) "Ne"))))

(ert-deftest sekken-delete/DEL_と_backspace_に割り当てる ()
  (should (eq (keymap-lookup sekken-mode-map "DEL") #'sekken-delete-backward))
  (should (eq (keymap-lookup sekken-mode-map "<backspace>") #'sekken-delete-backward)))

(ert-deftest sekken-delete/語を続けるので走る前にも後にも確定しない ()
  (with-temp-buffer
    (sekken-test-type "Nekoga")
    (cl-letf (((symbol-function 'sekken-convert-prefetch) #'ignore))
      (setq sekken-convert--cache '(("Nekoga" "猫が")))
      (let ((this-command #'sekken-delete-backward))
        (sekken-live-before-command)
        (sekken-delete-backward 1)
        (sekken-live-after-command)))
    (should (equal (buffer-string) "Neko"))
    (should (equal (sekken-word-bounds) (cons 1 5)))))

(provide 'sekken-delete-test)
;;; sekken-delete-test.el ends here
