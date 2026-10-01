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

(defmacro sekken-kana-test--with-table (tsv &rest body)
  "TSV の変換表で BODY を評価する。既定の表は書き換えない。"
  (declare (indent 1))
  `(let ((sekken-kana--table nil)
         (sekken-kana--max-key 1)
         (file (make-temp-file "sekken-kana-table" nil ".tsv" ,tsv)))
     (unwind-protect
         (progn (sekken-kana-load-table file) ,@body)
       (delete-file file))))

(ert-deftest sekken-kana/三列目の文字はキーの末尾として読み直す ()
  (sekken-kana-test--with-table "ka\tか\nkk\tっ\tk\n"
    (should (equal (sekken-kana-roman-to-kana "kk") "っk"))
    (should (equal (sekken-kana-roman-to-kana "kka") "っか"))
    (should (equal (sekken-kana-roman-to-kana "kkka") "っっか"))))

(ert-deftest sekken-kana/キーの末尾でない三列目の行は使わない ()
  (sekken-kana-test--with-table "ka\tか\nkk\tっ\ta\n"
    (should (equal (sekken-kana-roman-to-kana "kka") "kか"))))

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

(ert-deftest sekken-kana/表の全キーがどの位置で分かれても境目を検出する ()
  (maphash
   (lambda (key _kana)
     (cl-loop for split from 1 below (length key)
              do (should (sekken-kana-key-across-p
                          (concat "neko" (substring key 0 split))
                          (concat (substring key split) "neko")))))
   (sekken-kana--table))
  (should-not (sekken-kana-key-across-p "neko" ""))
  (should-not (sekken-kana-key-across-p "neko" "XYZ")))

(ert-deftest sekken-kana/境目の探索では最大キー長に応じた数の文字列だけを切り出す ()
  (sekken-kana--table)
  (let ((substring-function (symbol-function 'substring))
        (count 0))
    (cl-letf (((symbol-function 'substring)
               (lambda (&rest args)
                 (cl-incf count)
                 (apply substring-function args))))
      (should-not (sekken-kana-key-across-p "neko" "XYZ")))
    (should (<= count (* 2 sekken-kana--max-key sekken-kana--max-key)))))

(provide 'sekken-kana-test)
;;; sekken-kana-test.el ends here
