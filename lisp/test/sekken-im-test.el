;;; sekken-im-test.el --- sekken input method tests -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken)

(defvar corfu-auto-commands)

(ert-deftest sekken-im/input-method_として登録される ()
  (let ((entry (assoc "japanese-sekken" input-method-alist)))
    (should entry)
    (should (equal (nth 3 entry) "-かな:-"))))

(ert-deftest sekken-im/有効化と無効化が_buffer_の機能を揃える ()
  (with-temp-buffer
    (activate-input-method "japanese-sekken")
    (unwind-protect
        (progn
          (should (equal current-input-method "japanese-sekken"))
          (should (equal current-input-method-title "-かな:-"))
          (should (eq input-method-function #'sekken-im-filter))
          (should (local-variable-p 'input-method-function))
          (should sekken-mode))
      (deactivate-input-method))
    (should-not sekken-mode)))

(ert-deftest sekken-im/別_buffer_へ入力方式が漏れない ()
  (let ((source (generate-new-buffer " *sekken source*"))
        (other (generate-new-buffer " *sekken other*")))
    (unwind-protect
        (progn
          (with-current-buffer source
            (activate-input-method "japanese-sekken"))
          (with-current-buffer other
            (should-not (eq input-method-function #'sekken-im-filter))))
      (kill-buffer source)
      (kill-buffer other))))

(ert-deftest sekken-im/major-mode_変更後も入力できる ()
  (with-temp-buffer
    (activate-input-method "japanese-sekken")
    (fundamental-mode)
    (should sekken-mode)
    (should (eq (key-binding (vector (sekken-im--event ?n)))
                #'sekken-im-self-insert))
    (deactivate-input-method)))

(ert-deftest sekken-im/印字可能文字を合成イベントにする ()
  (with-temp-buffer
    (sekken-mode 1)
    (let* ((events (sekken-im-filter ?n))
           (event (car events))
           (command (key-binding (vector event))))
      (should (eq command #'sekken-im-self-insert))
      (let ((last-command-event event)
            (this-command command))
        (call-interactively command))
      (should (equal (buffer-string) "n")))))

(ert-deftest sekken-im/read-only_では元のキーを返す ()
  (with-temp-buffer
    (setq buffer-read-only t)
    (should (equal (sekken-im-filter ?n) '(110)))))

(ert-deftest sekken-im/front-sticky_read-only_では元のキーを返す ()
  (with-temp-buffer
    (sekken-mode 1)
    (insert (propertize "x" 'read-only t 'front-sticky t))
    (goto-char (point-min))
    (should (equal (sekken-im-filter ?n) '(110)))
    (let ((inhibit-read-only t))
      (should-not (equal (sekken-im-filter ?n) '(110))))))

(ert-deftest sekken-im/一時キーマップが握るキーは元のキーを返す ()
  (with-temp-buffer
    (let ((overriding-terminal-local-map
           (let ((map (make-sparse-keymap)))
             (define-key map "5" #'digit-argument)
             map)))
      (should (equal (sekken-im-filter ?5) '(53))))))

(ert-deftest sekken-im/overriding-local-map_中は元のキーを返す ()
  (with-temp-buffer
    (let ((overriding-local-map (make-sparse-keymap)))
      (should (equal (sekken-im-filter ?n) '(110))))))

(ert-deftest sekken-im/corfu-auto_へ挿入コマンドを登録する ()
  (let ((was-bound (boundp 'corfu-auto-commands))
        (old-value (and (boundp 'corfu-auto-commands)
                        (symbol-value 'corfu-auto-commands))))
    (unwind-protect
        (progn
          (set 'corfu-auto-commands '(self-insert-command))
          (sekken-im-setup-corfu)
          (should (memq 'sekken-im-self-insert corfu-auto-commands)))
      (if was-bound
          (set 'corfu-auto-commands old-value)
        (makunbound 'corfu-auto-commands)))))

(ert-deftest sekken-im/挿入コマンドは選択範囲を置換する ()
  (should (eq (get 'sekken-im-self-insert 'delete-selection)
              'delete-selection-uses-region-p)))

(provide 'sekken-im-test)
;;; sekken-im-test.el ends here
