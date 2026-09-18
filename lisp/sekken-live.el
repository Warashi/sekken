;;; sekken-live.el --- 打鍵に合わせて 1 位候補を見せるライブ変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 打鍵のたびに入力中の語の候補を先読みし、overlay に 1 位候補を見せる。
;; 候補がまだ無い間は、覚えている先頭部分の 1 位候補に残りのかなを
;; 繋いで見せ、かなに戻さない。それも無い間と、辞書を引く区間の無い
;; 語は、かな（と綴りのままの英字）を見せる。
;;
;; 確定の打鍵は無い。語を続けるコマンド（文字の挿入、後退削除、undo、
;; 候補一覧が出ている間のすべて）以外のコマンドは、走る前に見せていた
;; ものをバッファに入れる。送信・保存・バッファの切り替えのようにポイントを動かさず
;; 入力を持ち去る経路も、これで確定を通る。語を続けるコマンドは走った
;; 後にポイントが語の末尾から離れていれば確定し、語を伸ばす・縮める
;; 打鍵と、語が置き換わったコマンドでは何もしない。1 位以外を選ぶには
;; `sekken-convert' を使う。

;;; Code:

(require 'sekken-convert)
(require 'sekken-input)
(require 'sekken-word)
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

(defun sekken-live-display (roman analysis)
  "入力中の ROMAN を overlay で見せる文字列。ANALYSIS は ROMAN の解析結果。
候補を覚えていれば 1 位候補。無ければ、覚えている先頭部分の 1 位候補に
残りのかな表示を繋ぐ。それも無ければ境界を ▽ で示したかな表示。
先頭部分の残りは別の入力なので、そこだけ解析し直す。"
  (or (car (sekken-convert-cached roman))
      (let ((base (sekken-live--base roman)))
        (and base
             (concat (cdr base)
                     (sekken-input-display (substring roman (length (car base)))))))
      (plist-get analysis :display)))

(defun sekken-live--prefetch (roman analysis)
  "辞書を引く ROMAN の候補を先読みし、届いたらこのバッファの overlay を張り直す。
ANALYSIS は ROMAN の解析結果。"
  (when (and (plist-get analysis :converts)
             (plist-get analysis :ready))
    (let ((buffer (current-buffer)))
      (sekken-convert-prefetch
       roman
       (lambda ()
         (when (buffer-live-p buffer)
           (with-current-buffer buffer
             (sekken-live-update))))))))

(defun sekken-live-update ()
  "ポイント直前の語に合わせて overlay を張り直し、候補が無ければ先読みする。
見せるものと先読みの可否は同じ解析から導く。"
  (let ((bounds (sekken-word-bounds)))
    (if (null bounds)
        (sekken-overlay-clear)
      (let* ((start (car bounds))
             (end (cdr bounds))
             (roman (buffer-substring-no-properties start end))
             (analysis (sekken-input-analyze roman)))
        (sekken-overlay-show start end (sekken-live-display roman analysis))
        (sekken-live--prefetch roman analysis)))))

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
completion-in-region の候補一覧が出ている間はどのコマンドも続ける。一覧を
動く・絞る・閉じるコマンドは corfu のような UI ごとに違うので、名前では
なく `completion-in-region-mode' で見る。それ以外は
`sekken-live-continue-commands' にあるか、このバッファで DEL か backspace に
割り当てられていれば続ける。org-mode のように major mode が後退削除を
別のコマンドに張り替えても、キーで見れば漏れない。"
  (or completion-in-region-mode
      (memq command sekken-live-continue-commands)
      (eq command (key-binding [?\d]))
      (eq command (key-binding [backspace]))))

(defun sekken-live--result (roman)
  "確定で ROMAN を置き換える文字列。置き換えないなら nil。
辞書を引く区間があれば 1 位候補（届いていなければ待って引く）、
無ければかなと綴り。境界だけの語と、エンジンの失敗は nil。"
  (let ((analysis (sekken-input-analyze roman)))
    (cond
     ((seq-empty-p (plist-get analysis :pieces)) nil)
     ((plist-get analysis :converts)
      (car (or (sekken-convert-cached roman)
               (condition-case nil
                   (sekken-convert--candidates roman)
                 (error nil)))))
     (t (plist-get analysis :text)))))

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
              (sekken-word-finish start roman)
              (when inside
                (goto-char (+ start (length text)))))
          (text-read-only nil))))))

(defun sekken-live-before-command ()
  "語を続けるコマンドなら入力中の語を覚え、それ以外なら確定する。`pre-command-hook' 用。"
  (if (sekken-live-continue-p this-command)
      (sekken-word-hold)
    (let ((word (sekken-word-end)))
      (when (and word (not buffer-read-only))
        (apply #'sekken-live--commit word)
        (sekken-overlay-clear)))))

(defun sekken-live-after-command ()
  "終わった語を確定し、overlay を張り直す。`post-command-hook' 用。
語が終わったかどうかは `sekken-word-settle' が決め、ここは返された語を
置き換えるだけにする。undo が確定を取り消していれば
`sekken-word-revive' が語に戻す。"
  (let ((word (sekken-word-settle)))
    (when (and word (not buffer-read-only))
      (apply #'sekken-live--commit word)))
  (sekken-word-revive)
  (sekken-live-update))

(provide 'sekken-live)
;;; sekken-live.el ends here
