;;; sekken-benchmark.el --- 入力と移動の同期処理を計測する -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; emacs -Q --batch -L lisp -l lisp/test/sekken-benchmark.el -f sekken-benchmark-run
;; エンジンの応答待ちで、直前の 32 入力に候補がある状態を再現する。
;; 描画とエンジンの処理時間は含めず、GC の設定は変更しない。

;;; Code:

(require 'benchmark)
(require 'sekken)

(defun sekken-benchmark--sample (roman cache command)
  "ROMAN と CACHE を用意し、COMMAND の前後の hook を含めて 1 操作を計測する。"
  (with-temp-buffer
    (sekken-mode 1)
    (let ((this-command 'self-insert-command))
      (insert roman))
    (let ((sekken-convert--cache cache)
          (sekken-convert--in-flight "benchmark-pending")
          (sekken-convert--wanted nil)
          (this-command command))
      (benchmark-run 1
        (sekken-live-before-command)
        (let ((last-command-event ?a))
          (call-interactively command))
        (sekken-live-after-command)))))

(defun sekken-benchmark--report (roman cache unit command repetitions)
  "ROMAN と CACHE に対する COMMAND の計測を REPETITIONS 回行い、UNIT と結果を出す。"
  (sekken-benchmark--sample roman cache command)
  (garbage-collect)
  (let* ((results (cl-loop repeat repetitions
                           collect (sekken-benchmark--sample roman cache command)))
         (times (mapcar #'car results)))
    (princ
     (format "%d\t%s\t%s\t%d\t%.3f\t%.3f\t%d\t%.3f\n"
             (length roman) unit command repetitions
             (* 1000 (/ (apply #'+ times) repetitions))
             (* 1000 (apply #'max times))
             (apply #'+ (mapcar #'cadr results))
             (* 1000 (apply #'+ (mapcar #'caddr results)))))))

(defun sekken-benchmark-run (&optional repetitions sizes)
  "SIZES 文字の入力の末尾で打鍵・削除・移動を REPETITIONS 回ずつ計測する。
既定は 100・400・1000 文字で各 5 回。準備は計測に含めない。
各操作は同じ入力から開始し、平均・最大の経過時間と GC の合計を出力する。"
  (let ((repetitions (or repetitions 5))
        (sizes (or sizes '(100 400 1000))))
    (sekken-kana--table)
    (princ (format "Emacs %s; byte-compiled=%s; gc-cons-threshold=%s; gc-cons-percentage=%s\n"
                   emacs-version
                   (byte-code-function-p (symbol-function 'sekken-word-bounds))
                   gc-cons-threshold gc-cons-percentage))
    (princ "chars\tunit\tcommand\tsamples\tmean-ms\tmax-ms\tgc-count\tgc-ms\n")
    (cl-letf (((symbol-function 'sekken-server-adapt) #'ignore))
      (dolist (size sizes)
        (dolist (unit '("NekogaSuki" "Neko'gitbranch;Ha" "Neko'git branch;Ha"))
          (let* ((roman (substring
                         (apply #'concat (make-list (1+ (/ size (length unit))) unit))
                         0 size))
                 (cache (cl-loop for n from size downto (max 1 (- size 31))
                                 collect (list (substring roman 0 n) "candidate"))))
            (dolist (command '(self-insert-command delete-backward-char backward-char))
              (sekken-benchmark--report roman cache unit command repetitions))))))))

(provide 'sekken-benchmark)
;;; sekken-benchmark.el ends here
