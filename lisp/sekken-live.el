;;; sekken-live.el --- 打鍵に合わせて 1 位候補を見せるライブ変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 打鍵のたびに入力中の語の候補を先読みし、overlay に 1 位候補を見せる。
;; 候補がまだ無い間と、辞書を引く区間の無い語は、かな（と綴りのままの
;; 英字）を見せる。
;;
;; 確定の打鍵は無い。ポイントが入力中の語の末尾から離れたら（空白や
;; 改行を打つ、移動する）、見せていたものをバッファに入れる。語を
;; 伸ばす・縮める打鍵と、候補の選択などで語が置き換わったコマンドでは
;; 何もしない。1 位以外を選ぶには `sekken-convert' を使う。

;;; Code:

(require 'sekken-convert)
(require 'sekken-input)
(require 'sekken-overlay)

(defun sekken-live-display (roman)
  "入力中の ROMAN を overlay で見せる文字列。
候補を覚えていれば 1 位候補、無ければ境界を ▽ で示したかな表示。"
  (or (car (sekken-convert-cached roman))
      (sekken-input-display roman)))

(defun sekken-live--prefetch (roman)
  "辞書を引く ROMAN の候補を先読みし、届いたらこのバッファの overlay を張り直す。"
  (when (and (sekken-input-converts-p roman)
             (sekken-input-ready-p roman))
    (let ((buffer (current-buffer)))
      (sekken-convert-prefetch
       roman
       (lambda ()
         (when (buffer-live-p buffer)
           (with-current-buffer buffer
             (sekken-live-update))))))))

(defun sekken-live-update ()
  "ポイント直前の語に合わせて overlay を張り直し、候補が無ければ先読みする。"
  (let ((bounds (sekken-input-bounds)))
    (if (null bounds)
        (sekken-overlay-clear)
      (let* ((start (car bounds))
             (end (cdr bounds))
             (roman (buffer-substring-no-properties start end)))
        (sekken-overlay-show start end (sekken-live-display roman))
        (sekken-live--prefetch roman)))))

(defvar-local sekken-live--pending nil
  "コマンドの前にポイント直前にあった入力中の語 (START END ROMAN)。")

(defun sekken-live-before-command ()
  "ポイント直前の入力中の語を覚える。`pre-command-hook' 用。"
  (let ((bounds (sekken-input-bounds)))
    (setq sekken-live--pending
          (and bounds
               (list (car bounds) (cdr bounds)
                     (buffer-substring-no-properties (car bounds) (cdr bounds)))))))

(defun sekken-live--left-p (start end roman)
  "START から END の語 ROMAN がそのまま残り、ポイントがその語を続ける位置に無いか。
語が置き換わっていれば（候補の選択、undo）nil。語の途中に戻ったのは離れたと見る。"
  (and (<= end (point-max))
       (equal (buffer-substring-no-properties start end) roman)
       (let ((bounds (sekken-input-bounds)))
         (not (and bounds
                   (= (car bounds) start)
                   (>= (point) end))))))

(defun sekken-live--result (roman)
  "確定で ROMAN を置き換える文字列。置き換えないなら nil。
辞書を引く区間があれば 1 位候補（届いていなければ待って引く）、
無ければかなと綴り。境界で終わる語と、エンジンの失敗は nil。"
  (cond
   ((and (sekken-input-has-boundary-p roman)
         (not (sekken-input-ready-p roman)))
    nil)
   ((sekken-input-converts-p roman)
    (car (or (sekken-convert-cached roman)
             (condition-case nil
                 (sekken-convert--candidates roman)
               (error nil)))))
   (t (sekken-input-literal roman))))

(defun sekken-live--commit (start end roman)
  "START から END の語 ROMAN を確定の文字列に置き換える。
ポイントが語の中にあれば置き換えた後ろに、外にあれば同じ位置に置く。"
  (let ((text (sekken-live--result roman)))
    (when text
      (let ((inside (and (>= (point) start) (<= (point) end))))
        (save-excursion
          (goto-char start)
          (delete-region start end)
          (insert text))
        (when inside
          (goto-char (+ start (length text))))))))

(defun sekken-live-after-command ()
  "覚えた語から離れていれば確定し、overlay を張り直す。`post-command-hook' 用。"
  (let ((pending sekken-live--pending))
    (setq sekken-live--pending nil)
    (when (and pending
               (not buffer-read-only)
               (apply #'sekken-live--left-p pending))
      (apply #'sekken-live--commit pending)))
  (sekken-live-update))

(provide 'sekken-live)
;;; sekken-live.el ends here
