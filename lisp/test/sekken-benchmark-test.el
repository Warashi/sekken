;;; sekken-benchmark-test.el --- 入力性能計測のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-benchmark)

(ert-deftest sekken-benchmark/エンジンを呼ばず各入力と操作の計測値を出す ()
  (let ((sekken-convert--cache '(("Neko" "猫")))
        (sekken-convert--in-flight nil)
        (sekken-convert--wanted nil)
        (sekken-convert-last-error nil)
        (threshold gc-cons-threshold)
        (percentage gc-cons-percentage)
        (requests 0))
    (cl-letf (((symbol-function 'sekken-server-henkan-async)
               (lambda (&rest _) (cl-incf requests))))
      (let* ((output (with-output-to-string (sekken-benchmark-run 1 '(40))))
             (lines (split-string output "\n" t))
             (rows (cddr lines)))
        (should (= (length rows) 9))
        (dolist (unit '("NekogaSuki" "Neko'gitbranch;Ha" "Neko'git branch;Ha"))
          (dolist (command '("self-insert-command" "delete-backward-char" "backward-char"))
            (let ((columns (split-string (pop rows) "\t")))
              (should (equal (seq-take columns 4) (list "40" unit command "1")))
              (should (= (length columns) 8))
              (dolist (value (nthcdr 4 columns))
                (should (string-match-p "\\`[0-9]+\\(?:\\.[0-9]+\\)?\\'" value))))))))
    (should (zerop requests))
    (should-not sekken-convert-last-error)
    (should (equal sekken-convert--cache '(("Neko" "猫"))))
    (should-not sekken-convert--in-flight)
    (should-not sekken-convert--wanted)
    (should (= gc-cons-threshold threshold))
    (should (= gc-cons-percentage percentage))))

(provide 'sekken-benchmark-test)
;;; sekken-benchmark-test.el ends here
