;;; sekken-learn-test.el --- sekken-learn のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-learn)

(ert-deftest sekken-learn/段落の折り返しをつないで句点で切る ()
  (should (equal (sekken-learn-sentences
                  "折り返された文は\nつないでから切る。次の文も\n数える。\n")
                 '("折り返された文はつないでから切る。" "次の文も数える。"))))

(ert-deftest sekken-learn/前後に空白のある文が続いても先へ進む ()
  ;; 空白を落とすときのマッチで次の位置を見失うと、同じ文を切り続けて終わらない。
  (should (equal (sekken-learn-sentences "  前に空白のある文。  後にも空白がある文。  ")
                 '("前に空白のある文。" "後にも空白がある文。"))))

(ert-deftest sekken-learn/英数字の継ぎ目には空白を挟む ()
  (should (equal (sekken-learn-sentences "設定は tasks.org\nfile に書いた。")
                 '("設定は tasks.org file に書いた。"))))

(ert-deftest sekken-learn/見出し・表・ドロワーで段落を区切り箇条書きは記号を落とす ()
  (should (equal (sekken-learn-sentences
                  (concat "* 見出しは数えない。\n"
                          ":PROPERTIES:\n"
                          "本文の一つ目の文。\n"
                          "| 表の中の文。 |\n"
                          "- 箇条書きの文は\n"
                          "  続きの行とつなぐ。\n"
                          "1. 番号付きも数える。\n"))
                 '("本文の一つ目の文。"
                   "箇条書きの文は続きの行とつなぐ。"
                   "番号付きも数える。"))))

(ert-deftest sekken-learn/句点で終わらない文と和文の字の無い文と短い文は数えない ()
  (should (equal (sekken-learn-sentences
                  (concat "句点で終わらない\n\n"
                          "abc def ghi。\n\n"
                          "短い。\n\n"
                          "絵文字 😀 を含む文。\n\n"
                          "これは数える文です。"))
                 '("これは数える文です。"))))

(ert-deftest sekken-learn/ディレクトリから拡張子の合うファイルを集めて隠しディレクトリは飛ばす ()
  (let ((dir (make-temp-file "sekken-learn" t)))
    (unwind-protect
        (progn
          (make-directory (expand-file-name "notes" dir))
          (make-directory (expand-file-name ".git" dir))
          (dolist (f '("a.org" "notes/b.md" "c.txt" ".git/d.org"))
            (with-temp-file (expand-file-name f dir) (insert "x")))
          (should (equal (sort (mapcar (lambda (f) (file-relative-name f dir))
                                       (sekken-learn--files (list dir)))
                               #'string<)
                         '("a.org" "notes/b.md"))))
      (delete-directory dir t))))

(ert-deftest sekken-learn/同じ文は一度だけ送り決めた数ごとに分けて送る ()
  (let ((dir (make-temp-file "sekken-learn" t))
        (sent nil)
        (sekken-learn-batch 2))
    (unwind-protect
        (progn
          (with-temp-file (expand-file-name "a.org" dir)
            (insert "一つ目の文です。\n\n二つ目の文です。\n\n一つ目の文です。\n"))
          (with-temp-file (expand-file-name "b.org" dir)
            (insert "三つ目の文です。\n"))
          (cl-letf (((symbol-function 'sekken-server-learn)
                     (lambda (sentences) (push sentences sent) (length sentences))))
            (should (= (sekken-learn-files (list dir)) 3)))
          (should (equal (nreverse sent)
                         '(("一つ目の文です。" "二つ目の文です。")
                           ("三つ目の文です。")))))
      (delete-directory dir t))))

(provide 'sekken-learn-test)
;;; sekken-learn-test.el ends here
