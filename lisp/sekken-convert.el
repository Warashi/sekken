;;; sekken-convert.el --- 変換コマンドと補完 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken-convert' は、ポイント直前の語が大文字境界を含まなければ
;; その場でひらがなに置き換え、含めば候補を completion-in-region に渡す。
;; corfu などの completion-in-region-function がそのまま候補 UI になる。
;; 同じ候補を capf としても提供し、corfu-auto で入力中に出せるようにする。

;;; Code:

(require 'cl-lib)
(require 'sekken-input)
(require 'sekken-server)

(defcustom sekken-convert-max-candidates 10
  "エンジンに要求する候補数。"
  :type 'integer
  :group 'sekken)

(defvar sekken-convert--cache nil
  "直近の (入力 . 候補)。補完テーブルは同じ文字列で何度も呼ばれる。")

(defun sekken-convert--candidates (roman)
  "ROMAN の変換候補。直近の結果と同じ入力なら引き直さない。"
  (if (equal roman (car sekken-convert--cache))
      (cdr sekken-convert--cache)
    (let ((candidates (sekken-server-henkan roman sekken-convert-max-candidates)))
      (setq sekken-convert--cache (cons roman candidates))
      candidates)))

(defun sekken-convert-table (string _pred action)
  "変換候補を素通しする completion table。

候補は漢字かなで入力はローマ字なので、通常の table だと補完スタイルに
全部落とされる。`all-completions' (ACTION が t) で無条件に全件返す。
`try-completion' は STRING をそのまま返し、入力が候補の断片に置き換わる
のを避ける。候補は呼ばれた時点の STRING で引き直す。corfu は popup 中に
capf を呼び直さず table を使い回すため、候補を閉じ込めると追従しない。"
  (cond
   ((eq action 'metadata)
    '(metadata (category . sekken)
               (display-sort-function . identity)
               (cycle-sort-function . identity)))
   ((eq (car-safe action) 'boundaries) nil)
   ((eq action t) (sekken-convert--candidates string))
   ((null action) string)
   ((eq action 'lambda) (and (member string (sekken-convert--candidates string)) t))
   (t nil)))

(defvar sekken-mode)

(defun sekken-convert--fallback-command ()
  "`sekken-mode' が無ければこのキーに割り当たっていたコマンド。"
  (let ((sekken-mode nil))
    (key-binding (this-command-keys-vector) t)))

(cl-defun sekken-convert ()
  "ポイント直前の語を変換する。
大文字境界が無ければひらがなに置き換え、あれば候補から選ぶ。
変換する入力が無ければ、このキーの元のコマンド（改行など）を実行する。"
  (interactive)
  (let ((bounds (sekken-input-bounds)))
    (unless bounds
      (let ((fallback (sekken-convert--fallback-command)))
        (if (and fallback (not (eq fallback #'sekken-convert)))
            (progn
              (setq this-command fallback)
              (call-interactively fallback))
          (user-error "sekken: 変換する入力がありません")))
      (cl-return-from sekken-convert nil))
    (let* ((start (car bounds))
           (end (cdr bounds))
           (roman (buffer-substring-no-properties start end)))
      (if (sekken-input-has-upper-p roman)
          (completion-in-region start end #'sekken-convert-table)
        (delete-region start end)
        (goto-char start)
        (insert (sekken-kana-roman-to-kana roman))))))

(defun sekken-completion-at-point ()
  "大文字境界を含む入力中の語の変換候補を capf として返す。"
  (let ((bounds (sekken-input-bounds)))
    (when (and bounds
               (sekken-input-has-upper-p
                (buffer-substring-no-properties (car bounds) (cdr bounds))))
      (list (car bounds) (cdr bounds) #'sekken-convert-table
            :exclusive 'no))))

(provide 'sekken-convert)
;;; sekken-convert.el ends here
