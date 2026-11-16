;;; sekken-typo-test.el --- sekken-typo のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-typo)

(ert-deftest sekken-typo/読めない英字が無ければ候補を作らない ()
  (should-not (sekken-typo-alternatives "kanji"))
  ;; 末尾の子音はまだ打ちかけなので打ち間違いとみなさない。
  (should-not (sekken-typo-alternatives "kak"))
  (should-not (sekken-typo-alternatives "")))

(ert-deftest sekken-typo/入れ替わりと余分な_1_文字と隣接キーを直した読みを候補にする ()
  (should (member "かんじ" (sekken-typo-alternatives "knaji")))
  (should (member "かんじ" (sekken-typo-alternatives "kanjhi")))
  (should (member "にほんご" (sekken-typo-alternatives "nihpngo"))))

(ert-deftest sekken-typo/候補は英字の残らない読みを重ねずに並べる ()
  (let ((alternatives (sekken-typo-alternatives "kanjhi")))
    (should (equal alternatives (delete-dups (copy-sequence alternatives))))
    (should-not (seq-some (lambda (kana) (string-match-p "[a-z]" kana))
                          alternatives))))

(ert-deftest sekken-typo/QWERTY_の隣のキーを同じ段から並べる ()
  (should (equal (sekken-typo--neighbors ?p) '(?o ?l)))
  (should (equal (seq-take (sekken-typo--neighbors ?g) 2) '(?f ?h))))

(provide 'sekken-typo-test)
;;; sekken-typo-test.el ends here
