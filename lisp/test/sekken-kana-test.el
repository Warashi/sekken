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

(ert-deftest sekken-kana/境目にまたがる_2_文字以上のキーがあるかを引く ()
  (should (sekken-kana-key-across-p "nekoz" "/"))
  (should (sekken-kana-key-across-p "z" "/"))
  (should-not (sekken-kana-key-across-p "neko" "/"))
  (should-not (sekken-kana-key-across-p "" "/"))
  ;; 語を終える空白も、キーを完成させるなら取り込む。
  (should (sekken-kana-key-across-p "nekoz" " "))
  ;; 1 文字のキーは境目をまたがない。
  (should-not (sekken-kana-key-across-p "neko" "-"))
  ;; 3 文字のキーはどの位置で切れても当たる。
  (should (sekken-kana-key-across-p "nekok" "ya"))
  (should (sekken-kana-key-across-p "nekoky" "a"))
  (should-not (sekken-kana-key-across-p "nekok" "ka")))

(ert-deftest sekken-kana/Rust_と共通の_fixture_と一致する ()
  "エディタが送るかなが変換結果を決めるので、Rust 側と同じ表で一致を確かめる。"
  (with-temp-buffer
    (insert-file-contents
     (expand-file-name "../share/kana-cases.tsv"
                       (file-name-directory
                        (symbol-file 'sekken-kana-roman-to-kana 'defun))))
    (goto-char (point-min))
    (let ((count 0))
      (while (re-search-forward "^\\([^\t\n]+\\)\t\\([^\t\n]+\\)$" nil t)
        (setq count (1+ count))
        (should (equal (cons (match-string 1)
                             (sekken-kana-roman-to-kana (match-string 1)))
                       (cons (match-string 1) (match-string 2)))))
      (should (> count 20)))))

(provide 'sekken-kana-test)
;;; sekken-kana-test.el ends here
