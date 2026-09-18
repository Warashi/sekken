;;; sekken-word-test.el --- sekken-word のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-word)
(require 'sekken-test)

(ert-deftest sekken-word/ポイント直前のローマ字と記号を語として切り出す ()
  (with-temp-buffer
    (sekken-test-type "hello WagahaihaNekodearu.")
    (should (equal (sekken-word-bounds) (cons 7 26)))
    (sekken-test-type " ")
    (should (null (sekken-word-bounds)))))

(ert-deftest sekken-word/打っていない文字は語にしない ()
  ;; org の PROPERTIES ドロワーのように、もとからバッファにある文字。
  (with-temp-buffer
    (insert ":END:")
    (should (null (sekken-word-bounds)))
    (sekken-test-type "ne")
    (should (equal (sekken-word-bounds) (cons 6 8)))))

(ert-deftest sekken-word/打ち始めより前に戻れば語は無い ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "ne")
    (goto-char 3)
    (should (null (sekken-word-bounds)))))

(ert-deftest sekken-word/自己挿入でない挿入では打ち始めにしない ()
  (with-temp-buffer
    (let ((this-command #'yank))
      (insert "neko")
      (sekken-word-after-change 1 5 0))
    (should (null (sekken-word-bounds)))))

(ert-deftest sekken-word/打ち始めより前を編集すれば忘れる ()
  (with-temp-buffer
    (insert "abc")
    (sekken-test-type "neko")
    (sekken-word-before-change 3 4)
    (delete-region 3 4)
    (should (null (sekken-word-bounds)))
    ;; 語の中の削除では忘れない。
    (sekken-test-type "ne")
    (sekken-word-before-change 8 9)
    (delete-region 8 9)
    (should (equal (sekken-word-bounds) (cons 7 8)))))

(ert-deftest sekken-word/忘れた後の打鍵は新しい打ち始めになる ()
  (with-temp-buffer
    (sekken-test-type "neko")
    (sekken-word-forget-origin)
    (should (null (sekken-word-bounds)))
    (sekken-test-type "ga")
    (should (equal (sekken-word-bounds) (cons 5 7)))))

(ert-deftest sekken-word/数字と記号も入力中の語に含める ()
  ;; 1on1 のような数字混じりの語を 1 語として打てるよう、空白以外の
  ;; 打てる文字はすべて語の一部にする。
  (dolist (roman '("Kyou1on1" "'1on1" "(Neko)" "\"Neko\"" "Neko#1" "a@b"))
    (with-temp-buffer
      (sekken-test-type roman)
      (should (equal (sekken-word-bounds) (cons 1 (point-max)))))))

(ert-deftest sekken-word/空白と改行で語が終わる ()
  (dolist (delimiter '(" " "\n" "\t"))
    (with-temp-buffer
      (sekken-test-type "Neko")
      (sekken-test-type delimiter)
      (should (null (sekken-word-bounds)))
      (sekken-test-type "Ga")
      (should (equal (sekken-word-bounds) (cons 6 8))))))

(ert-deftest sekken-word/literal_の中の空白は語を切らない ()
  ;; 日本語の文に英語を数語挟んでも、文は改行まで 1 語のまま。
  (with-temp-buffer
    (sekken-test-type "Kyouha'git branch")
    (should (equal (sekken-word-bounds) (cons 1 (point-max))))
    (sekken-test-type ";wo;tsukau")
    (should (equal (sekken-word-bounds) (cons 1 (point-max)))))
  (with-temp-buffer
    (sekken-test-type "'git branch")
    (should (equal (sekken-word-bounds) (cons 1 (point-max)))))
  ;; `''' はリテラルのアポストロフィなので literal は続く。
  (with-temp-buffer
    (sekken-test-type "'don''t stop")
    (should (equal (sekken-word-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-word/literal_を閉じた後の空白は語を切る ()
  (dolist (roman '("'git;" "'git'" "'git/" "/git"))
    (with-temp-buffer
      (sekken-test-type roman)
      (sekken-test-type " ")
      (should (null (sekken-word-bounds)))
      (sekken-test-type "Ga")
      (should (equal (sekken-word-bounds)
                     (cons (+ (length roman) 2) (point-max)))))))

(ert-deftest sekken-word/セミコロンを入力中の語に含める ()
  (with-temp-buffer
    (sekken-test-type ";shokai;kougi")
    (should (equal (sekken-word-bounds) (cons 1 (point-max))))))

(ert-deftest sekken-word/スラッシュと山括弧を入力中の語に含める ()
  (with-temp-buffer
    (sekken-test-type "Kyou/emacs;O>Kai")
    (should (equal (sekken-word-bounds) (cons 1 (point-max))))))

(provide 'sekken-word-test)
;;; sekken-word-test.el ends here
