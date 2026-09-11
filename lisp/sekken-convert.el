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

(defun sekken-convert--candidates (roman &optional cancel-on-input)
  "ROMAN の変換候補。直近の結果と同じ入力なら引き直さない。
CANCEL-ON-INPUT が non-nil なら打鍵で待つのをやめ、候補なしとして nil を返す。
中断した結果は覚えないので、次に呼ばれたときに引き直す。"
  (if (equal roman (car sekken-convert--cache))
      (cdr sekken-convert--cache)
    (let ((candidates (sekken-server-henkan roman sekken-convert-max-candidates
                                            cancel-on-input)))
      (unless (eq candidates sekken-server--input-canceled)
        (setq sekken-convert--cache (cons roman candidates))
        candidates))))

(defun sekken-convert--complete (string action cancel-on-input)
  "変換候補を素通しする completion table の本体。

候補は漢字かなで入力はローマ字なので、通常の table だと補完スタイルに
全部落とされる。`all-completions' (ACTION が t) で無条件に全件返す。
`try-completion' は STRING をそのまま返し、入力が候補の断片に置き換わる
のを避ける。候補は呼ばれた時点の STRING で引き直す。corfu は popup 中に
capf を呼び直さず table を使い回すため、候補を閉じ込めると追従しない。
CANCEL-ON-INPUT が non-nil なら、引き直しを打鍵で中断する。"
  (cond
   ((eq action 'metadata)
    '(metadata (category . sekken)
               (display-sort-function . identity)
               (cycle-sort-function . identity)))
   ((eq (car-safe action) 'boundaries) nil)
   ((eq action t)
    (when (sekken-input-ready-p string)
      (sekken-convert--candidates string cancel-on-input)))
   ((null action) string)
   ((eq action 'lambda)
    (and (member string (cdr sekken-convert--cache)) t))
   (t nil)))

(defun sekken-convert-table (string _pred action)
  "変換候補を素通しする completion table。応答を待つ。
明示的な変換（`sekken-convert'）で使う。"
  (sekken-convert--complete string action nil))

(defun sekken-convert-auto-table (string _pred action)
  "変換候補を素通しする completion table。打鍵で待つのをやめる。
corfu-auto など入力中の補完で使う。中断したときは候補なしになり、
次の idle で引き直される。"
  (sekken-convert--complete string action t))

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
      (if (sekken-input-has-boundary-p roman)
          (if (sekken-input-ready-p roman)
              (completion-in-region start end #'sekken-convert-table)
            (user-error "sekken: 変換境界の後に読みがありません"))
        (delete-region start end)
        (goto-char start)
        (insert (sekken-kana-roman-to-kana
                 (car (sekken-input-segment roman))))))))

(defun sekken-completion-at-point ()
  "変換境界を含む入力中の語の変換候補を capf として返す。"
  (let ((bounds (sekken-input-bounds)))
    (when (and bounds
               (sekken-input-ready-p
                (buffer-substring-no-properties (car bounds) (cdr bounds))))
      (list (car bounds) (cdr bounds) #'sekken-convert-auto-table
            :exclusive t
            :company-prefix-length t))))

(provide 'sekken-convert)
;;; sekken-convert.el ends here
