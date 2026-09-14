;;; sekken-live-test.el --- sekken-live のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-live)
(require 'sekken-test)

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
    (sekken-test-type "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (should (equal (sekken-live-test--display) "▽ねこ"))
        (should (equal (car (car requests)) "Neko"))))))

(ert-deftest sekken-live/候補が届けば_1_位候補を見せる ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (setq sekken-convert--cache '(("Neko" "猫" "ねこ")))
        (funcall (cdr (car requests)))
        (should (equal (sekken-live-test--display) "猫"))))))

(ert-deftest sekken-live/届いた候補が今の語と違えばその候補に末尾を繋いで先読みし直す ()
  (with-temp-buffer
    (sekken-test-type "Ne")
    (let (requests)
      (sekken-live-test--with-prefetch requests
        (sekken-live-update)
        (sekken-test-type "ko")
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
      (sekken-test-type (car case))
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (setq sekken-convert--cache '(("Neko" "猫")))
          (sekken-live-update)
          (should (equal (sekken-live-test--display) (cdr case)))
          (should (equal (car (car requests)) (car case))))))))

(ert-deftest sekken-live/覚えた語のうち最も長いものに繋ぐ ()
  (with-temp-buffer
    (sekken-test-type "Nekogasuki")
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
      (sekken-test-type roman)
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (setq sekken-convert--cache cache)
          (sekken-live-update)
          (should (equal (sekken-live-test--display) display)))))))

(ert-deftest sekken-live/縮めた語を覚えていればその候補を見せる ()
  (with-temp-buffer
    (sekken-test-type "Neko")
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
      (sekken-test-type (car case))
      (let (requests)
        (sekken-live-test--with-prefetch requests
          (sekken-live-update)
          (should (equal (sekken-live-test--display) (cdr case)))
          (should-not requests))))))

(ert-deftest sekken-live/語が無くなれば_overlay_を消す ()
  (with-temp-buffer
    (sekken-test-type "Neko")
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
      (sekken-test-type "Neko")
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
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫" "ねこ"))
      (sekken-live-test--command #'self-insert-command (insert " ")))
    (should (equal (buffer-string) "猫 "))
    (should (= (point) (point-max)))
    (should (null sekken-overlay--overlay))))

(ert-deftest sekken-live/改行でも確定して改行は残る ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'newline (insert "\n")))
    (should (equal (buffer-string) "猫\n"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/移動のコマンドは確定してから確定した文字列の上を動く ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'backward-char (backward-char 1)))
    (should (equal (buffer-string) "猫"))
    (should (= (point) (point-min)))))

(ert-deftest sekken-live/語を続けるコマンドの後にポイントが語の末尾から離れていれば確定する ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'sekken-convert (backward-char 1)))
    (should (equal (buffer-string) "猫"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-live/語を伸ばす打鍵では確定しない ()
  (with-temp-buffer
    (sekken-test-type "Nek")
    (sekken-live-test--with-cache '(("Nek" "ねk"))
      (sekken-live-test--command #'self-insert-command (insert "o")))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/語を縮める_backspace_では確定しない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'delete-backward-char (delete-char -1)))
    (should (equal (buffer-string) "Nek"))))

(ert-deftest sekken-live/コマンドが語を置き換えていれば重ねて確定しない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'undo
       (delete-region 1 5)
       (insert "ねこ")))
    (should (equal (buffer-string) "ねこ"))))

(ert-deftest sekken-live/コマンドが語の前を書き換えても語が残っていれば確定する ()
  ;; auto-fill や electric-indent は、打鍵で前の行を詰め直してから語を動かす。
  (with-temp-buffer
    (sekken-test-type "Neko")
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
    (insert "abc ")
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'self-insert-command
       (insert " ")
       (narrow-to-region 1 4)
       (goto-char (point-min))))
    (widen)
    (should (equal (buffer-string) "abc Neko "))))

(ert-deftest sekken-live/語を続けるコマンドではポイントが動かなければ確定しない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'sekken-convert nil))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/語を続けないコマンドはポイントが動かなくても走る前に確定する ()
  (with-temp-buffer
    (sekken-test-type "Neko")
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
    (sekken-test-type "Neko")
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
      (sekken-test-type "Neko")
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
      (sekken-test-type "Neko")
      (sekken-live-test--with-cache '(("Neko" "猫"))
        (sekken-live-test--command command
          (should (equal (buffer-string) "Neko"))
          (delete-char -1)))
      (should (equal (buffer-string) "Nek")))))

(ert-deftest sekken-live/読み取り専用なら走る前に確定せずエラーも出さない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (setq buffer-read-only t)
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/読み取り専用の文字なら走る前に確定せずエラーも出さない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (put-text-property 1 5 'read-only t)
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/候補が届いていなければ待って引く ()
  (with-temp-buffer
    (sekken-test-type "Neko")
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
    (sekken-test-type "Neko")
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
      (sekken-test-type (car case))
      (sekken-live-test--with-cache nil
        (sekken-live-test--command #'self-insert-command (insert " ")))
      (should (equal (buffer-string) (cdr case))))))

(ert-deftest sekken-live/数字混じりの語も_1_語として置き換わる ()
  (dolist (case '(("'1on1" . "1on1 ")
                  ("Kyou1on1" . "今日1on1 ")))
    (with-temp-buffer
      (sekken-test-type (car case))
      (sekken-live-test--with-cache '(("Kyou1on1" "今日1on1"))
        (sekken-live-test--command #'self-insert-command (insert " ")))
      (should (equal (buffer-string) (cdr case))))))

(ert-deftest sekken-live/境界で終わる語は確定しない ()
  (with-temp-buffer
    (sekken-test-type "Neko;")
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'self-insert-command (insert " ")))
    (should (equal (buffer-string) "Neko; "))))

(ert-deftest sekken-live/確定した英字を直後の語として拾い直さない ()
  ;; 保存や M-x のように、ポイントを動かさず確定する経路が 2 回続く。
  (with-temp-buffer
    (sekken-test-type "kyouha'Emacs")
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'ignore)
      (should (null sekken-overlay--overlay))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "きょうはEmacs"))))

(ert-deftest sekken-live/確定した英字の後に打った語だけを変換する ()
  (with-temp-buffer
    (sekken-test-type "kyouha'Emacs")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'ignore)
      (sekken-live-test--command #'self-insert-command (sekken-test-type "Neko"))
      (should (equal (sekken-live-test--display) "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "きょうはEmacs猫"))))

(ert-deftest sekken-live/打っていない文字は語にせず_overlay_にも出さない ()
  (with-temp-buffer
    (insert "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-update)
      (should (null sekken-overlay--overlay))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "Neko"))))

(ert-deftest sekken-live/もとからある文字の後ろで確定しても壊さない ()
  ;; org の PROPERTIES ドロワーの :END: の後ろへ移動してコマンドを走らせる。
  (with-temp-buffer
    (insert ":END:")
    (sekken-live-test--with-cache '(("END:" "終わり"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) ":END:"))))

(ert-deftest sekken-live/もとからある文字に続けて打った語だけを変換する ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (should (equal (sekken-input-bounds) (cons 4 8)))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "abc猫"))))

(ert-deftest sekken-live/undo_で語が縮んでも残りは語のまま ()
  ;; self-insert は打鍵をまとめて undo するので、長い文の末尾がまとめて消える。
  (with-temp-buffer
    (sekken-test-type "Nekoga")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'undo (delete-region 5 7))
      (should (equal (sekken-input-bounds) (cons 1 5)))
      (should (equal (sekken-live-test--display) "猫")))))

(ert-deftest sekken-live/DEL_に割り当てられた_org_の後退削除でも確定しない ()
  (require 'org)
  (with-temp-buffer
    (org-mode)
    (should (eq (key-binding (kbd "DEL")) #'org-delete-backward-char))
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'org-delete-backward-char
        (should (equal (buffer-string) "Neko"))
        (delete-char -1)))
    (should (equal (buffer-string) "Nek"))))

(ert-deftest sekken-live/undo_で確定を取り消せば語に戻る ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'self-insert-command (insert " "))
      (should (equal (buffer-string) "猫 "))
      (sekken-live-test--command #'undo
        (delete-region 1 3)
        (insert "Neko"))
      (should (equal (sekken-input-bounds) (cons 1 5)))
      (should (equal (sekken-live-test--display) "猫"))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "猫"))))

(ert-deftest sekken-live/undo_で戻したローマ字が確定した語の先頭部分なら語に戻る ()
  ;; self-insert は打鍵を 20 文字ずつまとめて undo するので、長い語は先頭部分だけ戻る。
  (with-temp-buffer
    (sekken-test-type "Nekoga")
    (sekken-live-test--with-cache '(("Nekoga" "猫が") ("Nek" "ねk"))
      (sekken-live-test--command #'self-insert-command (insert " "))
      (should (equal (buffer-string) "猫が "))
      (sekken-live-test--command #'undo
        (delete-region 1 4)
        (insert "Nek"))
      (should (equal (sekken-input-bounds) (cons 1 4)))
      ;; もう一度 undo で語が空になっても、続けて打てば同じ語。
      (sekken-live-test--command #'undo (delete-region 1 4))
      (should (null (sekken-input-bounds)))
      (sekken-test-type "Ne")
      (should (equal (sekken-input-bounds) (cons 1 3))))))

(ert-deftest sekken-live/undo_で戻した文字が確定した語の先頭部分でなければ語に戻さない ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫"))
      (sekken-live-test--command #'self-insert-command (insert " "))
      (sekken-live-test--command #'undo
        (delete-region 1 3)
        (insert "Ne猫"))
      (should (null (sekken-input-bounds))))))

(ert-deftest sekken-live/redo_が綴りを戻してポイントを先頭に残せば末尾へ動かして語に戻す ()
  (with-temp-buffer
    (sekken-mode 1)
    (sekken-test-type "Nekoga")
    (sekken-live-test--with-cache '(("Nekoga" "猫が") ("Nek" "ねk"))
      (sekken-live-test--command #'self-insert-command (insert " "))
      (sekken-live-test--command #'undo (delete-region 1 3))
      (should (equal (buffer-string) " "))
      ;; redo は挿入を戻してもポイントを挿入位置の先頭に置く。
      (sekken-live-test--command #'undo-redo
        (goto-char 1)
        (let ((this-command #'undo-redo))
          (insert "Nek"))
        (goto-char 1))
      (should (= (point) 4))
      (should (equal (sekken-input-bounds) (cons 1 4)))
      ;; 先頭へ移動しただけでは動かさない。
      (sekken-live-test--command #'beginning-of-line (goto-char 1))
      (should (= (point) 1))
      (should (null (sekken-input-bounds))))
    (sekken-mode -1)))

(ert-deftest sekken-live/打鍵をまとめた本物の_undo_でも戻った先頭部分が語になる ()
  ;; コマンドループを真似て undo の境界を入れ、self-insert のまとめ方をそのまま通す。
  (require 'ert-x)
  (cl-flet ((run (command &optional char)
              (let ((last-command-event (or char last-command-event)))
                (ert-simulate-command (if char (list command 1) (list command)))
                (undo-auto--add-boundary))))
    (with-temp-buffer
      (buffer-enable-undo)
      (sekken-mode 1)
      (sekken-live-test--with-cache '(("WagahaihaNekodearu.Namaehamadanai." "吾輩は猫である。名前はまだ無い。"))
        (dolist (char (string-to-list "WagahaihaNekodearu.Namaehamadanai."))
          (run #'self-insert-command char))
        (run #'self-insert-command ?\s)
        (should (equal (buffer-string) "吾輩は猫である。名前はまだ無い。 "))
        (run #'undo)
        (should (equal (buffer-string) "WagahaihaNekodearu.Na"))
        (should (equal (sekken-input-bounds) (cons 1 22)))
        (should (equal (sekken-live-test--display) "▽わがはいは▽ねこである。▽な"))
        (run #'undo)
        (should (equal (buffer-string) ""))
        (should (null (sekken-input-bounds)))
        ;; redo で戻ればポイントを末尾へ動かして語に戻す。
        (run #'undo-redo)
        (should (equal (buffer-string) "WagahaihaNekodearu.Na"))
        (should (= (point) 22))
        (should (equal (sekken-input-bounds) (cons 1 22)))
        ;; もう一度 redo すれば置き換えた状態に戻り、語ではない。
        (run #'undo-redo)
        (should (equal (buffer-string) "吾輩は猫である。名前はまだ無い。 "))
        (should (null (sekken-input-bounds))))
      (sekken-mode -1))))

(ert-deftest sekken-live/語が置き換わった後に打った文字は新しい語になる ()
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-live-test--with-cache '(("Neko" "猫") ("Ga" "が"))
      (sekken-live-test--command #'undo
       (delete-region 1 5)
       (insert "ねこ"))
      (should (null (sekken-input-bounds)))
      (sekken-test-type "Ga")
      (should (equal (sekken-input-bounds) (cons 3 5)))
      (sekken-live-test--command #'ignore))
    (should (equal (buffer-string) "ねこが"))))

(ert-deftest sekken-live/sekken-mode_は編集の_hook_も付け外しする ()
  (require 'sekken)
  (with-temp-buffer
    (sekken-mode 1)
    (should (memq #'sekken-input-before-change before-change-functions))
    (should (memq #'sekken-input-after-change after-change-functions))
    ;; hook が付いているので、自己挿入のコマンドとして入れれば打ち始めになる。
    (let ((this-command #'self-insert-command))
      (insert "kyouha'Emacs"))
    (sekken-live-test--with-cache nil
      (sekken-live-test--command #'ignore)
      (should (equal (buffer-string) "きょうはEmacs"))
      ;; 確定した文字を削っても語には戻らず、続けて打った文字が新しい語になる。
      (sekken-live-test--command #'delete-backward-char (delete-char -1))
      (should (null sekken-overlay--overlay))
      (sekken-live-test--command #'self-insert-command
        (let ((this-command #'self-insert-command))
          (insert "Ne")))
      (should (equal (sekken-input-bounds) (cons 9 11)))
      (should (equal (sekken-live-test--display) "▽ね")))
    (sekken-mode -1)
    (should-not (memq #'sekken-input-before-change before-change-functions))
    (should-not (memq #'sekken-input-after-change after-change-functions))))

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
