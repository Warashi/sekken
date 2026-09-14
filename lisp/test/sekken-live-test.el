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
        (setq sekken-convert--cache '(("Neko" "猫" "ねこ")))
        (funcall (cdr (car requests)))
        (should (equal (sekken-live-test--display) "猫"))))))

(ert-deftest sekken-live/届いた候補が今の語と違えばその候補に末尾を繋いで先読みし直す ()
  (with-temp-buffer
    (insert "Ne")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (insert "ko")
        (setq sekken-convert--cache '(("Ne" "根")))
        (funcall (cdr (car requests)))
        (should (equal (sekken-live-test--display) "根こ"))
        (should (equal (car (car requests)) "Neko"))))))

(ert-deftest sekken-live/打ち足した語の候補が届くまでは前の候補に末尾のかなを繋いで見せる ()
  (dolist (case '(("Nekoga" . "猫が")
                  ("Nekog" . "猫g")
                  ("NekoGa" . "猫▽が")
                  ("Neko'Emacs" . "猫▽'Emacs")))
    (with-temp-buffer
      (insert (car case))
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (setq sekken-convert--cache '(("Neko" "猫")))
          (sekken-live-update)
          (should (equal (sekken-live-test--display) (cdr case)))
          (should (equal (car (car requests)) (car case))))))))

(ert-deftest sekken-live/覚えた語のうち最も長いものに繋ぐ ()
  (with-temp-buffer
    (insert "Nekogasuki")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (setq sekken-convert--cache '(("Ne" "根") ("Nekoga" "猫が") ("Neko" "猫")))
        (sekken-live-update)
        (should (equal (sekken-live-test--display) "猫がすき"))))))

(ert-deftest sekken-live/母音を待つ子音で終わる語と候補の無い語には繋がない ()
  (pcase-dolist (`(,cache ,roman ,display)
                 '(((("Nekog" "猫g") ("Neko" "猫")) "Nekoga" "猫が")
                   ((("Nekon" "猫ん") ("Neko" "猫")) "Nekona" "猫な")
                   ((("Nekog") ("Neko" "猫")) "Nekoga" "猫が")
                   ((("Nekog" "猫g")) "Nekoga" "▽ねこが")))
    (with-temp-buffer
      (insert roman)
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (setq sekken-convert--cache cache)
          (sekken-live-update)
          (should (equal (sekken-live-test--display) display)))))))

(ert-deftest sekken-live/縮めた語を覚えていればその候補を見せる ()
  (with-temp-buffer
    (insert "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (setq sekken-convert--cache '(("Nekoga" "猫が") ("Neko" "猫")))
        (sekken-live-update)
        (should (equal (sekken-live-test--display) "猫"))))))

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

(defmacro sekken-live-test--command (command &rest body)
  "BODY を `this-command' が COMMAND のコマンドとして、前後の hook を挟んで実行する。"
  (declare (indent 1))
  `(let ((this-command ,command))
     (sekken-live-before-command)
     ,@body
     (sekken-live-after-command)))

(defmacro sekken-live-test--with-cache (cache &rest body)
  "候補の cache を CACHE にし、エンジンを呼ばない状態で BODY を実行する。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-convert-prefetch) #'ignore)
             ((symbol-function 'sekken-server-henkan)
              (lambda (&rest _) (error "エンジンを呼んではいけない"))))
     (setq sekken-convert--cache ,cache)
     ,@body))

(ert-deftest sekken-live/語の後に入力文字以外を打てば_1_位候補に置き換わる ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫" "ねこ"))
      (sekken-live-test--command #'self-insert-command (insert " ")))
    (should (equal (buffer-string) "猫 "))
    (should (= (point) (point-max)))
    (should (null sekken-overlay--overlay))))

(ert-deftest sekken-live/改行でも確定して改行は残る ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'newline (insert "\n")))
    (should (equal (buffer-string) "猫\n"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/移動のコマンドは確定してから確定した文字列の上を動く ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'backward-char (backward-char 1)))
    (should (equal (buffer-string) "猫"))
    (should (= (point) (point-min)))))

(ert-deftest sekken-live/語を続けるコマンドの後にポイントが語の末尾から離れていれば確定する ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'sekken-convert (backward-char 1)))
    (should (equal (buffer-string) "猫"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/語を伸ばす打鍵では確定しない ()
  (with-temp-buffer
    (insert "Nek")
    (sekken-live-test--with-cache '(("Nek" "ねk"))
      (sekken-live-test--command #'self-insert-command (insert "o")))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/語を縮める_backspace_では確定しない ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'delete-backward-char (delete-char -1)))
    (should (equal (buffer-string) "Nek"))))

(ert-deftest sekken-live/コマンドが語を置き換えていれば重ねて確定しない ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'undo
       (delete-region 1 5)
       (insert "ねこ")))
    (should (equal (buffer-string) "ねこ"))))

(ert-deftest sekken-live/コマンドが語の前を書き換えても語が残っていれば確定する ()
  ;; auto-fill や electric-indent は、打鍵で前の行を詰め直してから語を動かす。
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'self-insert-command
       (insert " ")
       (save-excursion
         (goto-char (point-min))
         (insert "\n"))))
    (should (equal (buffer-string) "\n猫 "))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/コマンドが語を範囲の外にしても壊れない ()
  (with-temp-buffer
    (insert "abc Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'self-insert-command
       (insert " ")
       (narrow-to-region 1 4)
       (goto-char (point-min))))
    (widen)
    (should (equal (buffer-string) "abc Neko "))))

(ert-deftest sekken-live/語を続けるコマンドではポイントが動かなければ確定しない ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'sekken-convert nil))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/語を続けないコマンドはポイントが動かなくても走る前に確定する ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore
        (should (equal (buffer-string) "猫"))
        (should (= (point) (point-max)))
        (should (null sekken-overlay--overlay))))
    (should (equal (buffer-string) "猫"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/語を持ち去る送信コマンドには確定した文字列が渡る ()
  ;; agent-shell-submit のように、バッファから入力を読んで消す。
  (with-temp-buffer
    (insert "Neko")
    (let (sent)
      (sekken-live-test--with-cache '(("Neko" "猫"))
        (sekken-live-test--command #'agent-shell-submit
          (setq sent (buffer-string))
          (delete-region (point-min) (point-max))))
      (should (equal sent "猫"))
      (should (equal (buffer-string) "")))))

(ert-deftest sekken-live/語を続けるコマンドは走る前に確定しない ()
  (dolist (command '(sekken-im-self-insert self-insert-command
                     delete-backward-char backward-delete-char-untabify
                     sekken-convert undo undo-redo))
    (with-temp-buffer
      (insert "Neko")
      (sekken-live-test--with-cache '(("Neko" "猫"))
        (sekken-live-test--command command
          (should (equal (buffer-string) "Neko"))
          (insert "g"))))))

(ert-deftest sekken-live/DEL_に割り当てられたコマンドは列挙に無くても走る前に確定しない ()
  ;; org-mode は DEL を org-delete-backward-char に張り替える。
  (with-temp-buffer
    (use-local-map (let ((map (make-sparse-keymap)))
                     (define-key map (kbd "DEL") #'sekken-live-test-delete)
                     (define-key map [backspace] #'sekken-live-test-backspace)
                     map))
    (dolist (command '(sekken-live-test-delete sekken-live-test-backspace))
      (erase-buffer)
      (insert "Neko")
      (sekken-live-test--with-cache '(("Neko" "猫"))
        (sekken-live-test--command command
          (should (equal (buffer-string) "Neko"))
          (delete-char -1)))
      (should (equal (buffer-string) "Nek")))))

(ert-deftest sekken-live/読み取り専用なら走る前に確定せずエラーも出さない ()
  (with-temp-buffer
    (insert "Neko")
    (setq buffer-read-only t)
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/読み取り専用の文字なら走る前に確定せずエラーも出さない ()
  (with-temp-buffer
    (insert "Neko")
    (put-text-property 1 5 'read-only t)
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/候補が届いていなければ待って引く ()
  (with-temp-buffer
    (insert "Neko")
    (let (called)
      (cl-letf (((symbol-function 'sekken-convert-prefetch) #'ignore)
                ((symbol-function 'sekken-server-henkan)
                 (lambda (pieces _top &optional cancel-on-input)
                   (setq called (list pieces cancel-on-input))
                   '("猫"))))
        (setq sekken-convert--cache nil)
        (sekken-live-test--command #'self-insert-command (insert " ")))
      (should (equal called '([(:kind "convert" :text "ねこ")] nil))))
    (should (equal (buffer-string) "猫 "))))

(ert-deftest sekken-live/エンジンが失敗すればローマ字を残しキーの動作は妨げない ()
  (with-temp-buffer
    (insert "Neko")
    (cl-letf (((symbol-function 'sekken-convert-prefetch) #'ignore)
              ((symbol-function 'sekken-server-henkan)
               (lambda (&rest _) (signal 'jsonrpc-error '("dead")))))
      (setq sekken-convert--cache nil)
      (sekken-live-test--command #'self-insert-command (insert " ")))
    (should (equal (buffer-string) "Neko "))))

(ert-deftest sekken-live/辞書を引く区間の無い語はかなと綴りに置き換わる ()
  (dolist (case '(("kyouha" . "きょうは ")
                  ("kyouha'Emacs" . "きょうはEmacs ")))
    (with-temp-buffer
      (insert (car case))
      (sekken-live-test--with-cache nil
        (sekken-live-test--command #'self-insert-command (insert " ")))
      (should (equal (buffer-string) (cdr case))))))

(ert-deftest sekken-live/境界で終わる語は確定しない ()
  (with-temp-buffer
    (insert "Neko;")
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'self-insert-command (insert " ")))
    (should (equal (buffer-string) "Neko; "))))

(ert-deftest sekken-live/確定した英字を直後の語として拾い直さない ()
  ;; 保存や M-x のように、ポイントを動かさず確定する経路が 2 回続く。
  (with-temp-buffer
    (insert "kyouha'Emacs")
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'ignore)
      (should (null sekken-overlay--overlay))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "きょうはEmacs"))))

(ert-deftest sekken-live/確定した英字の後に打った語だけを変換する ()
  (with-temp-buffer
    (insert "kyouha'Emacs")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore)
      (sekken-live-test--command #'self-insert-command (insert "Neko"))
      (should (equal (sekken-live-test--display) "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "きょうはEmacs猫"))))

(ert-deftest sekken-live/sekken-mode_は編集の_hook_も付け外しする ()
  (require 'sekken)
  (with-temp-buffer
    (sekken-mode 1)
    (should (memq #'sekken-input-before-change before-change-functions))
    (insert "kyouha'Emacs")
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'ignore)
      (sekken-live-test--command #'delete-backward-char (delete-char -1))
      (should (equal (sekken-live-test--display) "▽えまc")))
    (sekken-mode -1)
    (should-not (memq #'sekken-input-before-change before-change-functions))))

(ert-deftest sekken-live/sekken-mode_が前後の_hook_を付け外しする ()
  (require 'sekken)
  (with-temp-buffer
    (sekken-mode 1)
    (should (memq #'sekken-live-before-command pre-command-hook))
    (should (memq #'sekken-live-after-command post-command-hook))
    (sekken-mode -1)
    (should-not (memq #'sekken-live-before-command pre-command-hook))
    (should-not (memq #'sekken-live-after-command post-command-hook))))

(provide 'sekken-live-test)
;;; sekken-live-test.el ends here
