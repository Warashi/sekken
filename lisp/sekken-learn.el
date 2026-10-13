;;; sekken-learn.el --- 過去に書いた文を本人の n-gram に数える -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken-learn-files' は、これまでに書いた org・Markdown の文をエンジンの本人の n-gram に
;; まとめて数える。本人の n-gram は変換で確定した文から数えるので、使い始めは空で効かない。
;; 過去の文を先に数えておくと、本人の語（技術語や言い回し）を最初から選べる
;; （sekken-train の experiments/2026-10-13-personal-seed: 英数字を含む自分の文 73.9% → 80.1%、
;; 代わりに Wikipedia の文は −2.1 ポイント）。
;;
;; 文は段落（空行・見出し・表・ドロワー・箇条書きの頭で区切る）の折り返しをつなぎ直し、
;; 「。」で切る。和文の字・ASCII の印字可能文字・和文の句読点だけからなり、和文の字を含む
;; 文を数える。同じ文を何度数えても数は足されるので、同じファイルを 2 度数えない。

;;; Code:

(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'sekken-server)

(defcustom sekken-learn-extensions '("org" "md")
  "`sekken-learn-files' がディレクトリから集めるファイルの拡張子。"
  :type '(repeat string)
  :group 'sekken)

(defcustom sekken-learn-batch 1000
  "`sekken-learn-files' が 1 回の要求でエンジンに送る文の数。"
  :type 'natnum
  :group 'sekken)

(defconst sekken-learn--allowed "\\`[ -~ぁ-ゖァ-ヺー々一-鿿、。]+\\'"
  "和文の字・ASCII の印字可能文字・和文の句読点だけからなる文字列。")

(defun sekken-learn--paragraphs (text)
  "TEXT を段落（行のリスト）に分ける。
空行・見出し（*）・コメント（#）・ドロワーやプロパティ（:）・表（|）で区切り、
箇条書きの頭（- + 1.）では新しい段落を始めて記号を落とす。"
  (let (paragraphs current)
    (dolist (line (split-string text "\n"))
      (let ((s (string-trim line)))
        (cond
         ((or (string-empty-p s)
              (memq (aref s 0) '(?* ?# ?: ?|))
              (string-prefix-p "Co-Authored" s))
          (when current (push (nreverse current) paragraphs))
          (setq current nil))
         ((string-match "\\`[ \t]*\\(?:[-+]\\|[0-9]+\\.\\)[ \t]+" line)
          (when current (push (nreverse current) paragraphs))
          (setq current (list (string-trim (substring line (match-end 0))))))
         (t (push s current)))))
    (when current (push (nreverse current) paragraphs))
    (nreverse paragraphs)))

(defun sekken-learn--ascii-alnum-p (char)
  "CHAR が ASCII の英数字か。"
  (and char (< char 128)
       (or (<= ?a char ?z) (<= ?A char ?Z) (<= ?0 char ?9))))

(defun sekken-learn--join (lines)
  "折り返された LINES をつなぐ。継ぎ目の両側が ASCII の英数字なら空白を 1 つ挟む。
org の fill は和文の間に空白を入れないので、それ以外は何も挟まない。"
  (let ((parts (list (car lines)))
        (prev (car lines)))
    (dolist (next (cdr lines))
      (when (and (not (string-empty-p prev)) (not (string-empty-p next))
                 (sekken-learn--ascii-alnum-p (aref prev (1- (length prev))))
                 (sekken-learn--ascii-alnum-p (aref next 0)))
        (push " " parts))
      (push next parts)
      ;; 継ぎ目は、つないだ結果の末尾（直前の空でない行の末尾）で見る。
      (unless (string-empty-p next)
        (setq prev next)))
    (apply #'concat (nreverse parts))))

(defun sekken-learn--keep-p (sentence)
  "SENTENCE を数えるか。6 字以上で、和文の字を含み、和文の字・ASCII の印字可能文字・
和文の句読点だけからなる文。"
  (and (>= (length sentence) 6)
       (string-match-p "[ぁ-ゖァ-ヺ一-鿿]" sentence)
       (string-match-p sekken-learn--allowed sentence)))

(defun sekken-learn-sentences (text)
  "org・Markdown の TEXT から数える文を取り出す。"
  (let (sentences)
    (dolist (paragraph (sekken-learn--paragraphs text))
      (let ((joined (sekken-learn--join paragraph))
            (start 0))
        (while (string-match "。" joined start)
          ;; `string-trim' はマッチの情報を書き換えるので、次の位置を先に取る。
          (let* ((end (match-end 0))
                 (sentence (string-trim (substring joined start end))))
            (when (sekken-learn--keep-p sentence)
              (push sentence sentences))
            (setq start end)))))
    (nreverse sentences)))

(defun sekken-learn--files (paths)
  "PATHS のファイルと、ディレクトリなら中の `sekken-learn-extensions' のファイル。
ディレクトリは再帰的に辿り、名前が . で始まるディレクトリ（.git など）は飛ばす。"
  (let ((regexp (concat "\\." (regexp-opt sekken-learn-extensions) "\\'")))
    (seq-mapcat
     (lambda (path)
       (if (file-directory-p path)
           (directory-files-recursively
            path regexp nil
            (lambda (dir)
              (not (string-prefix-p "." (file-name-nondirectory
                                         (directory-file-name dir))))))
         (list path)))
     paths)))

;;;###autoload
(defun sekken-learn-files (paths)
  "PATHS のファイルの文を、エンジンの本人の n-gram に数える。
対話的にはファイルかディレクトリを 1 つ聞く。ディレクトリなら中の
`sekken-learn-extensions' のファイルを再帰的に数える。1 回の呼び出しの中で
同じ文は 1 度だけ数える。数えた文の数を返す。"
  (interactive (list (list (read-file-name "数えるファイルかディレクトリ: "))))
  (let ((files (sekken-learn--files (mapcar #'expand-file-name paths)))
        (seen (make-hash-table :test #'equal))
        (pending nil)
        (npending 0)
        (count 0))
    (cl-flet ((send ()
                (when pending
                  (setq count (+ count (sekken-server-learn (nreverse pending))))
                  (setq pending nil
                        npending 0))))
      (dolist (file files)
        (dolist (sentence (sekken-learn-sentences
                           (with-temp-buffer
                             (insert-file-contents file)
                             (buffer-string))))
          (unless (gethash sentence seen)
            (puthash sentence t seen)
            (push sentence pending)
            (setq npending (1+ npending))
            (when (>= npending sekken-learn-batch)
              (send)))))
      (send))
    (message "sekken: %d ファイルから %d 文を数えました" (length files) count)
    count))

(provide 'sekken-learn)
;;; sekken-learn.el ends here
