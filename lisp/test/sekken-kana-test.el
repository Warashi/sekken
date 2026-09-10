;;; sekken-kana-test.el --- sekken-kana のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-kana)

(ert-deftest sekken-kana/基本的なローマ字をひらがなにする ()
  (should (equal (sekken-kana-roman-to-kana "nihongo") "にほんご"))
  (should (equal (sekken-kana-roman-to-kana "wagahai") "わがはい")))

(ert-deftest sekken-kana/促音と撥音を扱う ()
  (should (equal (sekken-kana-roman-to-kana "kitte") "きって"))
  (should (equal (sekken-kana-roman-to-kana "kannji") "かんじ"))
  (should (equal (sekken-kana-roman-to-kana "kanji") "かんじ")))

(ert-deftest sekken-kana/記号を全角にする ()
  (should (equal (sekken-kana-roman-to-kana "dearu.") "である。")))

(ert-deftest sekken-kana/表に無い文字はそのまま残す ()
  (should (equal (sekken-kana-roman-to-kana "q") "q")))

(ert-deftest sekken-kana/表示用は末尾の子音を変換せずに残す ()
  (should (equal (sekken-kana-display "nek") "ねk"))
  (should (equal (sekken-kana-display "kan") "かん"))
  (should (equal (sekken-kana-display "neko") "ねこ"))
  (should (equal (sekken-kana-display "kougi") "こうぎ")))

(provide 'sekken-kana-test)
;;; sekken-kana-test.el ends here
