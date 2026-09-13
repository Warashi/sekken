;;; sekken-convert-test.el --- sekken-convert のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-convert)

(defmacro sekken-convert-test--with-engine (candidates &rest body)
  "エンジン呼び出しを CANDIDATES を返す偽物に差し替えて BODY を実行する。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-server-henkan)
              (lambda (_input _top &optional _cancel-on-input) ,candidates)))
     (setq sekken-convert--cache nil)
     ,@body))

(ert-deftest sekken-convert/大文字が無ければひらがなに置き換える ()
  (with-temp-buffer
    (insert "kyouha")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "きょうは"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-convert/辞書を引く区間が無ければエンジンを呼ばずに置き換える ()
  (with-temp-buffer
    (insert "kyouha'Emacs")
    (sekken-convert-test--with-engine (error "エンジンを呼んではいけない")
      (sekken-convert))
    (should (equal (buffer-string) "きょうはEmacs"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-convert/開いたばかりの区間で終わる語は置き換えずに知らせる ()
  (dolist (roman '("neko'" "'" "Neko;" "Kyouha'"))
    (with-temp-buffer
      (insert roman)
      (sekken-convert-test--with-engine (error "エンジンを呼んではいけない")
        (should-error (sekken-convert) :type 'user-error))
      (should (equal (buffer-string) roman)))))

(ert-deftest sekken-convert/大文字があれば候補を_completion-in-region_に渡す ()
  (with-temp-buffer
    (insert "Neko")
    (let (called)
      (sekken-convert-test--with-engine '("猫" "ねこ")
        (cl-letf (((symbol-function 'completion-in-region)
                   (lambda (start end table &optional _pred)
                     (setq called
                           (list start end
                                 (all-completions
                                  (buffer-substring-no-properties start end)
                                  table))))))
          (sekken-convert)))
      (should (equal called '(1 5 ("猫" "ねこ")))))))

(ert-deftest sekken-convert/入力が無ければ元のキーのコマンドを実行する ()
  (with-temp-buffer
    (insert "abc ")
    (cl-letf (((symbol-function 'sekken-convert--fallback-command)
               (lambda () #'newline)))
      (sekken-convert))
    ;; `newline' は electric-indent-mode で直前の空白を消す。
    (should (equal (buffer-string) "abc\n"))))

(ert-deftest sekken-convert/補完テーブルは入力に関係なく候補を全件返す ()
  (sekken-convert-test--with-engine '("猫" "ねこ")
    (should (equal (all-completions "Neko" #'sekken-convert-table) '("猫" "ねこ")))
    (should (equal (try-completion "Neko" #'sekken-convert-table) "Neko"))
    (should (test-completion "猫" #'sekken-convert-table))))

(ert-deftest sekken-convert/capf_の_table_は打鍵で中断し中断した結果を覚えない ()
  (let (calls)
    (cl-letf (((symbol-function 'sekken-server-henkan)
               (lambda (input _top &optional cancel-on-input)
                 (push (list input cancel-on-input) calls)
                 (if cancel-on-input
                     sekken-server--input-canceled
                   '("猫")))))
      (setq sekken-convert--cache nil)
      ;; 中断されると候補なし。
      (should (null (all-completions "Neko" #'sekken-convert-auto-table)))
      (should (null sekken-convert--cache))
      ;; 明示的な変換は待って候補を得る。
      (should (equal (all-completions "Neko" #'sekken-convert-table) '("猫")))
      ;; 同じ入力なら中断する版も引き直さない。
      (should (equal (all-completions "Neko" #'sekken-convert-auto-table) '("猫")))
      (should (equal (reverse calls)
                     '(([(:kind "convert" :text "ねこ")] t)
                       ([(:kind "convert" :text "ねこ")] nil)))))))

(ert-deftest sekken-convert/capf_は辞書を引く区間を含む語だけに反応する ()
  (with-temp-buffer
    (insert "neko'emacs")
    (should (null (sekken-completion-at-point))))
  (with-temp-buffer
    (insert "neko")
    (should (null (sekken-completion-at-point)))
    (insert " Neko")
    (let ((capf (sekken-completion-at-point)))
      (should (= (nth 0 capf) 6))
      (should (= (nth 1 capf) 10))
      (should (eq (nth 2 capf) #'sekken-convert-auto-table))
      (should (eq (plist-get (nthcdr 3 capf) :exclusive) t))
      (should (eq (plist-get (nthcdr 3 capf) :company-prefix-length) t)))))

(defmacro sekken-convert-test--with-async-engine (requests &rest body)
  "非同期のエンジン呼び出しを、要求を REQUESTS に溜めるだけの偽物にして BODY を実行する。
REQUESTS の各要素は (PIECES . ON-SUCCESS)。返事は呼び出し側が ON-SUCCESS で返す。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-server-henkan-async)
              (lambda (pieces _top on-success &optional _on-failure)
                (push (cons pieces on-success) ,requests))))
     (setq sekken-convert--cache nil
           sekken-convert--in-flight nil
           sekken-convert--wanted nil)
     ,@body))

(ert-deftest sekken-convert/先読みは候補を覚えてから知らせる ()
  (let (requests notified)
    (sekken-convert-test--with-async-engine requests
      (sekken-convert-prefetch "Neko" (lambda () (setq notified t)))
      (should (equal (car (car requests)) [(:kind "convert" :text "ねこ")]))
      (should-not notified)
      (funcall (cdr (car requests)) '("猫" "ねこ"))
      (should notified)
      (should (equal (sekken-convert-cached "Neko") '("猫" "ねこ"))))))

(ert-deftest sekken-convert/覚えている入力は先読みしない ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (setq sekken-convert--cache '("Neko" "猫"))
      (sekken-convert-prefetch "Neko" #'ignore)
      (should-not requests))))

(ert-deftest sekken-convert/飛ばしている間の入力は最後の_1_つだけ返事の後に送る ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (sekken-convert-prefetch "Ne" #'ignore)
      (sekken-convert-prefetch "Nek" #'ignore)
      (sekken-convert-prefetch "Neko" #'ignore)
      (should (= (length requests) 1))
      (funcall (cdr (car requests)) '("ね"))
      (should (= (length requests) 2))
      (should (equal (car (car requests)) [(:kind "convert" :text "ねこ")]))
      (funcall (cdr (car requests)) '("猫"))
      (should (= (length requests) 2))
      (should (equal (sekken-convert-cached "Neko") '("猫"))))))

(ert-deftest sekken-convert/先読みが失敗すれば次の入力を送れる ()
  (let (requests failures)
    (cl-letf (((symbol-function 'sekken-server-henkan-async)
               (lambda (pieces _top _on-success on-failure)
                 (push pieces requests)
                 (push on-failure failures))))
      (setq sekken-convert--cache nil
            sekken-convert--in-flight nil
            sekken-convert--wanted nil)
      (sekken-convert-prefetch "Ne" #'ignore)
      (sekken-convert-prefetch "Nek" #'ignore)
      (funcall (car failures))
      (should-not sekken-convert--in-flight)
      (should (= (length requests) 1))
      (sekken-convert-prefetch "Neko" #'ignore)
      (should (= (length requests) 2)))))

(ert-deftest sekken-convert/先読みの起動失敗は握りつぶす ()
  (cl-letf (((symbol-function 'sekken-server-henkan-async)
             (lambda (&rest _) (user-error "設定がありません"))))
    (setq sekken-convert--cache nil
          sekken-convert--in-flight nil
          sekken-convert--wanted nil)
    (sekken-convert-prefetch "Neko" #'ignore)
    (should-not sekken-convert--in-flight)))

(ert-deftest sekken-convert/sticky_境界を変換候補に渡す ()
  (with-temp-buffer
    (insert ";shokai;kougi")
    (let (called)
      (sekken-convert-test--with-engine '("初回講義")
        (cl-letf (((symbol-function 'completion-in-region)
                   (lambda (start end table &optional _pred)
                     (setq called
                           (list (buffer-substring-no-properties start end)
                                 (all-completions
                                  (buffer-substring-no-properties start end)
                                  table))))))
          (sekken-convert)))
      (should (equal called '(";shokai;kougi" ("初回講義")))))))

(ert-deftest sekken-convert/sticky_待機中は候補を出さない ()
  (with-temp-buffer
    (insert ";")
    (should-not (sekken-completion-at-point))
    (sekken-convert-test--with-engine
        (error "境界だけでエンジンを呼んではならない")
      (should (equal (all-completions ";" #'sekken-convert-table) nil)))
    (should-error (sekken-convert) :type 'user-error)))

(ert-deftest sekken-convert/二重セミコロンはリテラルとして確定する ()
  (with-temp-buffer
    (insert "semi;;koron")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "せみ;ころん"))))

(provide 'sekken-convert-test)
;;; sekken-convert-test.el ends here
