;;; sekken-convert.el --- 変換コマンドと補完 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken-convert' は、ポイント直前の語に辞書を引く区間が無ければ
;; その場でかな（と綴りのままの英字）に置き換え、あれば候補を
;; completion-in-region に渡す。
;; corfu などの completion-in-region-function がそのまま候補 UI になる。
;; 候補は sekken-live が打鍵ごとに先読みして覚え、ここと共有する。

;;; Code:

(require 'cl-lib)
(require 'sekken-input)
(require 'sekken-word)
(require 'sekken-server)

(defcustom sekken-convert-max-candidates 10
  "エンジンに要求する候補数。"
  :type 'integer
  :group 'sekken)

(defconst sekken-convert--cache-size 32
  "覚えておく (入力 . 候補) の件数。")

(defvar sekken-convert--cache nil
  "引いた順に新しいものから並ぶ (入力 . 候補) の alist。
補完テーブルは同じ文字列で何度も呼ばれ、ライブ変換は語を縮めたときに
前に引いた語をもう一度見せる。")

(defun sekken-convert--remember (roman candidates)
  "ROMAN の候補 CANDIDATES を覚える。同じ入力の古い候補は捨てる。"
  (setq sekken-convert--cache
        (seq-take (cons (cons roman candidates)
                        (assoc-delete-all roman sekken-convert--cache))
                  sekken-convert--cache-size)))

(defun sekken-convert--candidates (roman)
  "ROMAN の変換候補。覚えている入力なら引き直さない。応答を待つ。"
  (or (sekken-convert-cached roman)
      (let ((candidates (sekken-server-henkan (sekken-input-pieces roman)
                                              sekken-convert-max-candidates)))
        (sekken-convert--remember roman candidates)
        candidates)))

(defun sekken-convert-forget-all ()
  "覚えている候補をすべて捨てる。辞書が変わったときに引き直すため。"
  (setq sekken-convert--cache nil))

(defun sekken-convert-cached (roman)
  "ROMAN の候補を覚えていればそのリスト。無ければ nil。"
  (cdr (assoc roman sekken-convert--cache)))

(defvar sekken-convert--in-flight nil
  "非同期に引いている最中の入力。飛ばすのは同時に 1 本まで。")

(defvar sekken-convert--wanted nil
  "飛ばしている間に頼まれた (入力 . 知らせる関数)。返事が来たら最後の 1 つだけ送る。")

(defvar sekken-convert-last-error nil
  "最後に先読みが失敗した (入力 . 理由)。
先読みは打鍵のたびに走るので、失敗を出すと打っている最中に邪魔になる。
自動確定は候補を待たずにかなで確定するので、失敗しても入力は壊れない。
その代わり、なぜ候補が来なかったかをここから読めるようにする。")

(defun sekken-convert--failed (roman reason)
  "ROMAN の先読みが REASON で失敗したことを残す。"
  (setq sekken-convert-last-error (cons roman reason)))

(defun sekken-convert-prefetch (roman notify)
  "ROMAN の候補を非同期に引いて覚え、届いたら NOTIFY を引数なしで呼ぶ。
既に覚えていれば何もしない。別の入力を引いている最中なら覚えておき、
その返事の後に送る。失敗は打鍵を止めず、`sekken-convert-last-error' に残す。"
  (cond
   ((sekken-convert-cached roman) nil)
   ((equal sekken-convert--in-flight roman) nil)
   (sekken-convert--in-flight
    (setq sekken-convert--wanted (cons roman notify)))
   (t
    (setq sekken-convert--in-flight roman)
    (condition-case err
        (sekken-server-henkan-async
         (sekken-input-pieces roman) sekken-convert-max-candidates
         (lambda (candidates)
           (setq sekken-convert--in-flight nil)
           (sekken-convert--remember roman candidates)
           ;; 溜めた入力を先に送る。NOTIFY が同じ入力を頼み直しても飛ばさない。
           (let ((wanted sekken-convert--wanted))
             (setq sekken-convert--wanted nil)
             (when wanted
               (sekken-convert-prefetch (car wanted) (cdr wanted))))
           (funcall notify))
         ;; 失敗しても溜めた入力は送り直さない。次の打鍵が送る。
         (lambda ()
           (setq sekken-convert--in-flight nil
                 sekken-convert--wanted nil)
           (sekken-convert--failed roman "エンジンが変換を返しませんでした")))
      (error
       (setq sekken-convert--in-flight nil
             sekken-convert--wanted nil)
       (sekken-convert--failed roman (error-message-string err)))))))

(defun sekken-convert-table (string _pred action)
  "変換候補を素通しする completion table。

候補は漢字かなで入力はローマ字なので、通常の table だと補完スタイルに
全部落とされる。`all-completions' (ACTION が t) で無条件に全件返す。
`try-completion' は STRING をそのまま返し、入力が候補の断片に置き換わる
のを避ける。候補は呼ばれた時点の STRING で引き直す。corfu は popup 中に
table を使い回すため、候補を閉じ込めると追従しない。"
  (cond
   ((eq action 'metadata)
    '(metadata (category . sekken)
               (display-sort-function . identity)
               (cycle-sort-function . identity)))
   ((eq (car-safe action) 'boundaries) nil)
   ((eq action t)
    (unless (seq-empty-p (sekken-input-pieces string))
      (sekken-convert--candidates string)))
   ((null action) string)
   ((eq action 'lambda)
    (and (member string (cdr (car sekken-convert--cache))) t))
   (t nil)))

(defvar sekken-mode)

(defun sekken-convert--fallback-command ()
  "`sekken-mode' が無ければこのキーに割り当たっていたコマンド。"
  (let ((sekken-mode nil))
    (key-binding (this-command-keys-vector) t)))

(defun sekken-convert--finish (start roman)
  "候補が入ったら START からの語 ROMAN を終える exit-function を返す。
exit-function が呼ばれた時点で候補はバッファに入っているので、status は
見ない。corfu は選んだ候補を入れて一覧を抜けるとき finished ではなく
exact で呼ぶ。選んだ候補はエンジンに学習させる。"
  (lambda (string _status)
    (sekken-word-finish start roman)
    (sekken-server-adapt (sekken-input-pieces roman) string)))

(cl-defun sekken-convert ()
  "ポイント直前の語を変換する。
辞書を引く区間が無ければかなと綴りに置き換え、あれば候補から選ぶ。
変換する入力が無ければ、このキーの元のコマンド（改行など）を実行する。"
  (interactive)
  (let ((bounds (sekken-word-bounds)))
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
           (roman (buffer-substring-no-properties start end))
           (analysis (sekken-input-analyze roman)))
      (cond
       ((seq-empty-p (plist-get analysis :pieces))
        (user-error "sekken: 変換境界の後に読みがありません"))
       ((plist-get analysis :converts)
        ;; corfu は開始時点の `completion-extra-properties' を保存して
        ;; 選択時に使うので、動的束縛で足りる。
        (let ((completion-extra-properties
               (list :exit-function (sekken-convert--finish start roman))))
          (completion-in-region start end #'sekken-convert-table)))
       (t
        (delete-region start end)
        (goto-char start)
        (insert (plist-get analysis :commit))
        (sekken-word-finish start roman))))))

(provide 'sekken-convert)
;;; sekken-convert.el ends here
