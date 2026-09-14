;;; sekken-live.el --- 打鍵に合わせて 1 位候補を見せるライブ変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 打鍵のたびに入力中の語の候補を先読みし、overlay に 1 位候補を見せる。
;; 候補がまだ無い間は、覚えている先頭部分の 1 位候補に残りのかなを
;; 繋いで見せ、かなに戻さない。それも無い間と、辞書を引く区間の無い
;; 語は、かな（と綴りのままの英字）を見せる。
;;
;; 確定の打鍵は無い。語を続けるコマンド（文字の挿入、後退削除、候補の
;; 選択、undo）以外のコマンドは、走る前に見せていたものをバッファに
;; 入れる。送信・保存・バッファの切り替えのようにポイントを動かさず
;; 入力を持ち去る経路も、これで確定を通る。語を続けるコマンドは走った
;; 後にポイントが語の末尾から離れていれば確定し、語を伸ばす・縮める
;; 打鍵と、語が置き換わったコマンドでは何もしない。1 位以外を選ぶには
;; `sekken-convert' を使う。

;;; Code:

(require 'sekken-convert)
(require 'sekken-input)
(require 'sekken-overlay)

(defun sekken-live--base (roman)
  "ROMAN の候補が届くまで代わりに見せる (先頭部分 . 1 位候補)。無ければ nil。
候補を覚えている最も長い先頭部分を選ぶ。母音を待つ子音で終わる先頭部分は、
残りをかなにできないので選ばない。"
  (let ((length (1- (length roman)))
        base)
    (while (and (> length 0) (not base))
      (let ((prefix (substring roman 0 length)))
        (unless (string-match-p "[bcdfghjklmnpqrstvwxyz]\\'" (downcase prefix))
          (let ((candidate (car (sekken-convert-cached prefix))))
            (when candidate
              (setq base (cons prefix candidate))))))
      (setq length (1- length)))
    base))

(defun sekken-live-display (roman)
  "入力中の ROMAN を overlay で見せる文字列。
候補を覚えていれば 1 位候補。無ければ、覚えている先頭部分の 1 位候補に
残りのかな表示を繋ぐ。それも無ければ境界を ▽ で示したかな表示。"
  (or (car (sekken-convert-cached roman))
      (let ((base (sekken-live--base roman)))
        (and base
             (concat (cdr base)
                     (sekken-input-display (substring roman (length (car base)))))))
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

(defcustom sekken-live-continue-commands
  '(sekken-im-self-insert self-insert-command
    delete-backward-char backward-delete-char-untabify
    sekken-convert undo undo-redo undo-only)
  "入力中の語を続けるので、走る前に確定しないコマンド。
これに加えて、そのバッファで DEL と backspace に割り当てられたコマンドも
後退削除として語を続ける（`sekken-live-continue-p'）。それ以外のコマンドは、
ポイントを動かさなくても走る前に語を確定する。"
  :type '(repeat function)
  :group 'sekken)

(defun sekken-live-continue-p (command)
  "COMMAND が入力中の語を続けるか。
`sekken-live-continue-commands' にあるか、このバッファで DEL か backspace に
割り当てられていれば続ける。org-mode のように major mode が後退削除を
別のコマンドに張り替えても、キーで見れば漏れない。"
  (or (memq command sekken-live-continue-commands)
      (eq command (key-binding [?\d]))
      (eq command (key-binding [backspace]))))

(defvar-local sekken-live--pending nil
  "語を続けるコマンドの前にポイント直前にあった入力中の語 (START END ROMAN)。
START と END は marker で、auto-fill や electric-indent がコマンドの中で
語の前を書き換えても語を追える。START は直前への挿入で進み、END は進まない
ので、語を伸ばした打鍵は範囲の外に出る。")

(defun sekken-live--forget ()
  "覚えた語を忘れ、marker を外す。"
  (when sekken-live--pending
    (set-marker (nth 0 sekken-live--pending) nil)
    (set-marker (nth 1 sekken-live--pending) nil)
    (setq sekken-live--pending nil)))

(defun sekken-live-before-command ()
  "語を続けるコマンドなら入力中の語を覚え、それ以外なら確定する。`pre-command-hook' 用。"
  (sekken-live--forget)
  (let ((bounds (sekken-input-bounds)))
    (if (sekken-live-continue-p this-command)
        (when bounds
          (setq sekken-live--pending
                (list (copy-marker (car bounds) t)
                      (copy-marker (cdr bounds))
                      (buffer-substring-no-properties (car bounds) (cdr bounds)))))
      (when (and bounds (not buffer-read-only))
        (sekken-live--commit (car bounds) (cdr bounds)
                             (buffer-substring-no-properties (car bounds) (cdr bounds)))
        (sekken-overlay-clear))
      ;; 語が無くても、語を続けないコマンドの後に打つ文字は新しい語。
      (sekken-input-forget-origin))))

(defun sekken-live--intact-p (start end roman)
  "START から END の語 ROMAN がそのまま残っているか。
候補の選択や undo で置き換わっていれば nil。"
  (and (>= start (point-min))
       (<= end (point-max))
       (equal (buffer-substring-no-properties start end) roman)))

(defun sekken-live--shrunk-p (start roman)
  "START から始まる語 ROMAN が、ポイントまでの先頭部分だけ残って縮んだか。
まとめて打った末尾を undo で消したときで、置き換わったのとは違い語は続く。"
  (and (>= start (point-min))
       (>= (point) start)
       (string-prefix-p (buffer-substring-no-properties start (point)) roman)))

(defun sekken-live--continuing-p (start end)
  "ポイントが START から END の語を続ける位置にあるか。
語の途中に戻ったのは離れたと見る。"
  (let ((bounds (sekken-input-bounds)))
    (and bounds
         (= (car bounds) start)
         (>= (point) end))))

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
ポイントが語の中にあれば置き換えた後ろに、外にあれば同じ位置に置く。
読み取り専用の文字に当たれば置き換えず、`pre-command-hook' から呼ばれても
コマンドを止めない。"
  (let ((text (sekken-live--result roman)))
    (when text
      (let ((inside (and (>= (point) start) (<= (point) end))))
        (condition-case nil
            (progn
              (save-excursion
                (goto-char start)
                (delete-region start end)
                (insert text))
              (sekken-input-finish-word start roman)
              (when inside
                (goto-char (+ start (length text)))))
          (text-read-only nil))))))

(defun sekken-live-after-command ()
  "覚えた語から離れていれば確定し、overlay を張り直す。`post-command-hook' 用。
語が置き換わっていれば確定はしないが、その語は終わったので打ち始めを忘れる。
縮んだだけなら語は続く。undo が確定を取り消していれば語に戻す。"
  (when sekken-live--pending
    (let ((start (marker-position (nth 0 sekken-live--pending)))
          (end (marker-position (nth 1 sekken-live--pending)))
          (roman (nth 2 sekken-live--pending)))
      (cond
       ((not (sekken-live--intact-p start end roman))
        (unless (sekken-live--shrunk-p start roman)
          (sekken-input-forget-origin)))
       ((sekken-live--continuing-p start end) nil)
       (t
        (unless buffer-read-only
          (sekken-live--commit start end roman))
        ;; 境界で終わる語のように置き換えなくても、離れた語は終わり。
        (sekken-input-forget-origin)))))
  (sekken-live--forget)
  (sekken-input-revive-word)
  (sekken-live-update))

(provide 'sekken-live)
;;; sekken-live.el ends here
