;;; sekken-register-test.el --- sekken-register のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-register)
(require 'sekken-test)

(ert-deftest sekken-register/読みはローマ字でもかなでも境界なしのかなになる ()
  (dolist (case '(("Warashi" . "わらし")
                  ("warashi" . "わらし")
                  ("わらし" . "わらし")
                  (";sawada;dazai" . "さわだだざい")
                  ("SawadaDazai" . "さわだだざい")))
    (should (equal (sekken-register-yomi (car case)) (cdr case)))))

(ert-deftest sekken-register/読みの既定値は最後に置き換えた語のかなで前の語は履歴に残る ()
  (with-temp-buffer
    (should-not (sekken-register-recent-yomi))
    (sekken-test-type "Warashi")
    (sekken-word-finish (point-min) "Warashi")
    (should (equal (sekken-register-recent-yomi) '("わらし")))
    ;; 外れた語を sekken で打ち直すと、直した語が最後になる。
    (sekken-test-type "Wara;Ichi")
    (sekken-word-finish (- (point) 9) "Wara;Ichi")
    (should (equal (sekken-register-recent-yomi) '("わらいち" "わらし")))))

(ert-deftest sekken-register/読みの既定値から末尾の記号を落とす ()
  (with-temp-buffer
    (sekken-test-type "Sawadadazai.")
    (sekken-word-finish (point-min) "Sawadadazai.")
    (sekken-test-type "...")
    (sekken-word-finish (- (point) 3) "...")
    (should (equal (sekken-register-recent-yomi) '("さわだだざい")))))

(ert-deftest sekken-register/前に置き換えた語の読みは履歴で聞く ()
  (with-temp-buffer
    (sekken-test-type "Warashi")
    (sekken-word-finish (point-min) "Warashi")
    (sekken-test-type "Wara;Ichi")
    (sekken-word-finish (- (point) 9) "Wara;Ichi")
    (let (registered history)
      (cl-letf (((symbol-function 'sekken-server-register)
                 (lambda (yomi surface) (setq registered (cons yomi surface))))
                ((symbol-function 'read-string)
                 (lambda (prompt &optional _initial hist default &rest _)
                   (if (string-prefix-p "読み" prompt)
                       (progn (setq history (symbol-value hist))
                              (car history))
                     "藁市"))))
        (call-interactively #'sekken-register))
      (should (equal history '("わらし")))
      (should (equal registered '("わらし" . "藁市"))))))

(ert-deftest sekken-register/読みをかなにして送り覚えている候補を捨てる ()
  (let ((sekken-convert--cache '(("Warashi" "童")))
        registered)
    (cl-letf (((symbol-function 'sekken-server-register)
               (lambda (yomi surface) (setq registered (cons yomi surface)))))
      (sekken-register "Warashi" "藁市"))
    (should (equal registered '("わらし" . "藁市")))
    (should-not sekken-convert--cache)))

(ert-deftest sekken-register/登録した語を確定と同じく学習に送る ()
  ;; 外れた語の確定を学習した後は、辞書に足しただけでは登録した語が
  ;; 1 位に戻らない。登録を「読みをその語に確定した」1 回分として学習させる。
  (let (adapted)
    (cl-letf (((symbol-function 'sekken-server-register) #'ignore)
              ((symbol-function 'sekken-server-adapt)
               (lambda (pieces sentence) (setq adapted (cons pieces sentence)))))
      (sekken-register "Warashi" "藁市"))
    (should (equal adapted '([(:kind "convert" :text "わらし")] . "藁市")))))

(ert-deftest sekken-register/regionがあればその文字列を語にする ()
  (with-temp-buffer
    (insert "藁市")
    (push-mark (point-min) t t)
    (goto-char (point-max))
    (sekken-test-type "Warashi")
    (sekken-word-finish (- (point-max) 7) "Warashi")
    (let ((transient-mark-mode t)
          registered prompts)
      (cl-letf (((symbol-function 'sekken-server-register)
                 (lambda (yomi surface) (setq registered (cons yomi surface))))
                ((symbol-function 'read-string)
                 (lambda (prompt &optional _initial _history default &rest _)
                   (push prompt prompts)
                   default)))
        (call-interactively #'sekken-register))
      (should (equal registered '("わらし" . "藁市Warashi")))
      (should (= (length prompts) 1)))))

(ert-deftest sekken-register/regionが無ければ語を聞く ()
  (with-temp-buffer
    (let (registered prompts)
      (cl-letf (((symbol-function 'sekken-server-register)
                 (lambda (yomi surface) (setq registered (cons yomi surface))))
                ((symbol-function 'read-string)
                 (lambda (prompt &optional _initial _history default &rest _)
                   (push prompt prompts)
                   (if (string-prefix-p "読み" prompt) "sawadadazai" "沢田太宰"))))
        (call-interactively #'sekken-register))
      (should (equal registered '("さわだだざい" . "沢田太宰")))
      (should (= (length prompts) 2)))))

;;; sekken-register-test.el ends here
