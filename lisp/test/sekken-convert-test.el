;;; sekken-convert-test.el --- sekken-convert のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-convert)
(require 'sekken-test)

(defmacro sekken-convert-test--with-engine (candidates &rest body)
  "エンジン呼び出しを CANDIDATES を返す偽物に差し替えて BODY を実行する。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-server-henkan)
              (lambda (_input _top) ,candidates)))
     (setq sekken-convert--cache nil)
     ,@body))

(ert-deftest sekken-convert/大文字が無ければひらがなに置き換える ()
  (with-temp-buffer
    (sekken-test-type "kyouha")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "きょうは"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-convert/辞書を引く区間が無ければエンジンを呼ばずに置き換える ()
  (with-temp-buffer
    (sekken-test-type "kyouha'Emacs")
    (sekken-convert-test--with-engine (error "エンジンを呼んではいけない")
      (sekken-convert))
    (should (equal (buffer-string) "きょうはEmacs"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-convert/境界だけの語は置き換えずに知らせる ()
  (dolist (roman '("'" ";" ">"))
    (with-temp-buffer
      (sekken-test-type roman)
      (sekken-convert-test--with-engine (error "エンジンを呼んではいけない")
        (should-error (sekken-convert) :type 'user-error))
      (should (equal (buffer-string) roman)))))

(ert-deftest sekken-convert/境界で終わる語は手前までを置き換える ()
  ;; 末尾の `;' `'' は前の区間を閉じただけなので、辞書を引く区間が無ければその場で置き換える。
  (dolist (case '(("neko'" . "ねこ") ("'git branch;" . "git branch")))
    (with-temp-buffer
      (sekken-test-type (car case))
      (sekken-convert-test--with-engine (error "エンジンを呼んではいけない")
        (sekken-convert))
      (should (equal (buffer-string) (cdr case)))))
  ;; 辞書を引く区間があれば、手前の語の候補を一覧に渡す。
  (with-temp-buffer
    (sekken-test-type "Neko;")
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
      (should (equal called '(1 6 ("猫" "ねこ")))))))

(ert-deftest sekken-convert/大文字があれば候補を_completion-in-region_に渡す ()
  (with-temp-buffer
    (sekken-test-type "Neko")
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

(ert-deftest sekken-convert/選んだ候補の末尾を確定として覚える ()
  (with-temp-buffer
    (insert "kyouha'Emacs ")
    (sekken-test-type "Neko")
    (sekken-convert-test--with-engine '("猫")
      (cl-letf (((symbol-function 'completion-in-region)
                 (lambda (start end _table &optional _pred)
                   (delete-region start end)
                   (goto-char start)
                   (insert "猫")
                   (funcall (plist-get completion-extra-properties :exit-function)
                            "猫" 'finished))))
        (sekken-convert)))
    (should (equal (buffer-string) "kyouha'Emacs 猫"))
    (should (null (sekken-word-bounds)))))

(ert-deftest sekken-convert/status_が_finished_でなくても候補が入れば語を終える ()
  ;; corfu は選んだ候補を入れて一覧を抜けるとき exact で呼ぶ。
  (with-temp-buffer
    (sekken-test-type "Neko")
    (sekken-convert-test--with-engine '("猫")
      (cl-letf (((symbol-function 'completion-in-region)
                 (lambda (start end _table &optional _pred)
                   (delete-region start end)
                   (goto-char start)
                   (insert "猫")
                   (funcall (plist-get completion-extra-properties :exit-function)
                            "猫" 'exact))))
        (sekken-convert)))
    (should (null (sekken-word-bounds)))
    (should (equal (sekken-word-finished-romans) '("Neko")))))

(ert-deftest sekken-convert/かなと綴りに置き換えた末尾を確定として覚える ()
  (with-temp-buffer
    (sekken-test-type "kyouha'Emacs")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "きょうはEmacs"))
    (should (null (sekken-word-bounds)))))

(ert-deftest sekken-convert/undo_で置き換えを取り消せば語に戻る ()
  (with-temp-buffer
    (sekken-test-type "kyouha")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (null (sekken-word-bounds)))
    (delete-region 1 5)
    (insert "kyouha")
    (sekken-word-revive)
    (should (equal (sekken-word-bounds) (cons 1 7)))))

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

(ert-deftest sekken-convert/複数の入力の候補を覚え_縮めた語も引ける ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (sekken-convert-prefetch "Ne" #'ignore)
      (funcall (cdr (car requests)) '("根"))
      (sekken-convert-prefetch "Neko" #'ignore)
      (funcall (cdr (car requests)) '("猫"))
      (should (equal (sekken-convert-cached "Ne") '("根")))
      (should (equal (sekken-convert-cached "Neko") '("猫")))
      (sekken-convert-prefetch "Ne" #'ignore)
      (should (= (length requests) 2)))))

(ert-deftest sekken-convert/同じ入力を引き直せば古い候補を捨てる ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (setq sekken-convert--cache '(("Neko") ("Ne" "根")))
      (sekken-convert-prefetch "Neko" #'ignore)
      (funcall (cdr (car requests)) '("猫"))
      (should (equal sekken-convert--cache '(("Neko" "猫") ("Ne" "根")))))))

(ert-deftest sekken-convert/覚える件数には上限がある ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (dotimes (i (1+ sekken-convert--cache-size))
        (let ((roman (format "Neko%d" i)))
          (sekken-convert-prefetch roman #'ignore)
          (funcall (cdr (car requests)) (list roman))))
      (should (= (length sekken-convert--cache) sekken-convert--cache-size))
      (should-not (sekken-convert-cached "Neko0"))
      (should (sekken-convert-cached (format "Neko%d" sekken-convert--cache-size))))))

(ert-deftest sekken-convert/覚えている入力は先読みしない ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (setq sekken-convert--cache '(("Neko" "猫")))
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

(ert-deftest sekken-convert/返事の中で同じ入力を頼み直しても二重に送らない ()
  (let (requests)
    (sekken-convert-test--with-async-engine requests
      (sekken-convert-prefetch "Ne"
                               (lambda () (sekken-convert-prefetch "Neko" #'ignore)))
      (sekken-convert-prefetch "Neko" #'ignore)
      (funcall (cdr (car requests)) '("ね"))
      (should (= (length requests) 2))
      (should (equal sekken-convert--in-flight "Neko"))
      (should-not sekken-convert--wanted))))

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
    (sekken-test-type ";shokai;kougi")
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
    (sekken-test-type ";")
    (sekken-convert-test--with-engine
        (error "境界だけでエンジンを呼んではならない")
      (should (equal (all-completions ";" #'sekken-convert-table) nil)))
    (should-error (sekken-convert) :type 'user-error)))

(ert-deftest sekken-convert/二重セミコロンはリテラルとして確定する ()
  (with-temp-buffer
    (sekken-test-type "semi;;koron")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "せみ;ころん"))))

(provide 'sekken-convert-test)
;;; sekken-convert-test.el ends here
