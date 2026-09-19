;;; sekken-word.el --- 入力中の語の開始・編集・確定・復元 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 入力中の語の生涯をこのバッファローカルな状態にまとめる。
;; 「入力中の語」は、自分が打ち始めた位置からポイントまでに連続する、
;; 空白以外の打てる文字（印字可能な ASCII）の並び。ただし literal 区間の
;; 中の空白は語を終えない。打ち始めは自己挿入のコマンドが文字を入れた位置で、
;; それより前には伸びない。もとからバッファにある文字や貼り付けた文字は
;; 打っていないので語にならず、確定の結果が英字で終わっても、それをもう一度
;; 語として拾わない。確定を undo で取り消してローマ字（かその先頭部分）が
;; 同じ位置に戻れば、打ち始めを復元して語に戻す。
;;
;; ローマ字をどう読むかは `sekken-input' が決め、語を何で置き換えて
;; どう見せるかは `sekken-live' と `sekken-convert' が決める。

;;; Code:

(require 'sekken-input)
(require 'subr-x)

(defun sekken-word--char-p (char)
  "CHAR が入力中の語を構成する文字か。
空白を除く印字可能な ASCII、つまり `sekken-im' が拾う文字のうち空白以外。
数字や括弧も語の一部にして、1on1 のような語を 1 語として打てるようにする。"
  (and (<= ?! char) (<= char ?~)))

(defcustom sekken-word-typing-commands
  '(sekken-im-self-insert self-insert-command)
  "文字を打つコマンド。これらが入れた文字だけを入力中の語にする。"
  :type '(repeat function)
  :group 'sekken)

(defvar-local sekken-word--origin nil
  "入力中の語の打ち始めの marker。無ければ語は無い。
marker は直前への挿入で進まないので、打ち始めの位置に打った文字は語に入る。")

(defun sekken-word--forget-origin ()
  "打ち始めを忘れる。次に打った文字が新しい語の打ち始めになる。"
  (when sekken-word--origin
    (set-marker sekken-word--origin nil)
    (setq sekken-word--origin nil)))

(defvar-local sekken-word--finished nil
  "最後に置き換えた語の (START . ROMAN)。START は marker。
undo が置き換えを取り消してローマ字が同じ位置に戻ったとき、語に戻すため。")

(defconst sekken-word--finished-limit 10
  "覚えておく、置き換えた語のローマ字の件数。")

(defvar-local sekken-word--finished-romans nil
  "置き換えた語のローマ字。新しい順で `sekken-word--finished-limit' 件まで。
外れた語を sekken で打ち直してから登録するとき、外れた語の読みに戻るため。")

(defun sekken-word-finish (start roman)
  "START から始まっていた語 ROMAN を置き換え終えたことにする。
打ち始めを忘れ、次に打った文字は新しい語になる。"
  (sekken-word--forget-origin)
  (when sekken-word--finished
    (set-marker (car sekken-word--finished) nil))
  (setq sekken-word--finished (cons (copy-marker start) roman)
        sekken-word--finished-romans
        (seq-take (cons roman (delete roman sekken-word--finished-romans))
                  sekken-word--finished-limit)))

(defun sekken-word-finished-romans ()
  "このバッファで置き換えた語のローマ字。新しい順。無ければ nil。"
  sekken-word--finished-romans)

(defun sekken-word--forget-finished ()
  "最後に置き換えた語を忘れる。"
  (when sekken-word--finished
    (set-marker (car sekken-word--finished) nil)
    (setq sekken-word--finished nil)))

(defvar-local sekken-word--restored nil
  "打つコマンド以外が、最後に置き換えた語の位置に戻した綴りの末尾。
redo は挿入を戻してもポイントを挿入位置の先頭に置くので、コマンドの後に
ポイントがそこに残っていれば、ここまで動かして語に戻す。")

(defun sekken-word--restored-p (beg end)
  "BEG から END への挿入が、最後に置き換えた語の位置に綴りの先頭部分を戻したか。"
  (and sekken-word--finished
       (marker-buffer (car sekken-word--finished))
       (= beg (marker-position (car sekken-word--finished)))
       (<= (- end beg) (length (cdr sekken-word--finished)))
       (string-prefix-p (buffer-substring-no-properties beg end)
                        (cdr sekken-word--finished))))

(defun sekken-word-revive ()
  "最後に置き換えた語のローマ字の先頭部分が同じ位置に戻り、ポイントがその中にあれば語に戻す。
undo が置き換えを取り消したときに、打ち始めからその語を続けられるようにする。
self-insert は打鍵を 20 文字ずつまとめて undo するので、戻るのは綴り全体とは
限らず先頭部分になる。redo のようにコマンドが綴りを戻してポイントをその先頭に
残していれば、末尾へ動かしてから語に戻す。打ち始めがあれば何もしない。"
  (let ((restored sekken-word--restored))
    (setq sekken-word--restored nil)
    (when (and sekken-word--finished
               (marker-buffer (car sekken-word--finished)))
      (let ((start (marker-position (car sekken-word--finished)))
            (roman (cdr sekken-word--finished)))
        ;; undo で語が空まで縮んだ後は打ち始めが残ったままなので、
        ;; ポイントを動かすのは打ち始めの有無によらない。
        (when (and restored (= (point) start))
          (goto-char restored))
        (when (and (null sekken-word--origin)
                   (> (point) start)
                   (<= (- (point) start) (length roman))
                   (string-prefix-p (buffer-substring-no-properties start (point)) roman))
          (setq sekken-word--origin (copy-marker start)))))))

(defun sekken-word-after-change (beg end old-length)
  "文字を打つコマンドが BEG に文字を入れたら、打ち始めが無ければそこを打ち始めにする。
`after-change-functions' 用。OLD-LENGTH が 0 でない置き換えと、
END が BEG の削除は打ち始めにしない。打つコマンド以外が最後に置き換えた語の
綴りを戻したなら、その末尾を覚える（`sekken-word-revive'）。"
  (when (and (zerop old-length)
             (< beg end))
    (if (memq this-command sekken-word-typing-commands)
        (unless sekken-word--origin
          (setq sekken-word--origin (copy-marker beg)))
      (when (sekken-word--restored-p beg end)
        (setq sekken-word--restored end)))))

(defun sekken-word-before-change (beg _end)
  "BEG から始まる編集が打ち始めより前に触れば打ち始めを忘れる。
`before-change-functions' 用。打ち始めへの挿入と、それより後ろの削除は
語の中の編集なので忘れない。削除は marker を動かすので、動く前に見る。"
  (when (and sekken-word--origin
             (< beg sekken-word--origin))
    (sekken-word--forget-origin)))

(defun sekken-word--continues-p (roman char)
  "ROMAN の後に打った空白などの CHAR が語を終えずに続くか。
literal 区間の中の空白と、直前までと合わせて変換表のキーになる文字
（`z' の後の空白は全角空白のキー）は語に含める。"
  (let* ((segmented (sekken-input-segment roman))
         (last (car (last (cdr segmented)))))
    (cond
     ((plist-get last :literal) t)
     ((plist-get last :abbrev) nil)
     (t (sekken-kana-key-across-p
         (if last (plist-get last :text) (car segmented))
         (string char))))))

(defun sekken-word-bounds ()
  "ポイント直前の入力中の語の (START . END)。無ければ nil。
打ち始めより前には伸びない。空白は語を終えるが、literal 区間の中の空白と
変換表のキーを完成させる空白は語に含める（`sekken-word--continues-p'）。
日本語の文に英語を数語挟んでも文を 1 語のまま保つため。"
  (when (and sekken-word--origin
             (< sekken-word--origin (point)))
    (let* ((end (point))
           (start (marker-position sekken-word--origin))
           (index start))
      ;; 打ち始めから走査し、literal の外の空白の直後を語の始まりにする。
      (while (< index end)
        (unless (or (sekken-word--char-p (char-after index))
                    (sekken-word--continues-p
                     (buffer-substring-no-properties start index)
                     (char-after index)))
          (setq start (1+ index)))
        (setq index (1+ index)))
      (when (< start end)
        (cons start end)))))

(defvar-local sekken-word--pending nil
  "コマンドが走る前にポイント直前にあった入力中の語 (START END ROMAN)。
START と END は marker で、auto-fill や electric-indent がコマンドの中で
語の前を書き換えても語を追える。START は直前への挿入で進み、END は進まない
ので、語を伸ばした打鍵は範囲の外に出る。")

(defun sekken-word--forget-pending ()
  "覚えた語を忘れ、marker を外す。"
  (when sekken-word--pending
    (set-marker (nth 0 sekken-word--pending) nil)
    (set-marker (nth 1 sekken-word--pending) nil)
    (setq sekken-word--pending nil)))

(defun sekken-word--current ()
  "ポイント直前の入力中の語 (START END ROMAN)。無ければ nil。"
  (let ((bounds (sekken-word-bounds)))
    (when bounds
      (list (car bounds) (cdr bounds)
            (buffer-substring-no-properties (car bounds) (cdr bounds))))))

(defun sekken-word-hold ()
  "入力中の語を、コマンドが走る間も追えるように覚える。
語を続けるコマンドが走る前に呼ぶ。その後は `sekken-word-settle' が見る。"
  (sekken-word--forget-pending)
  (let ((word (sekken-word--current)))
    (when word
      (setq sekken-word--pending
            (list (copy-marker (nth 0 word) t)
                  (copy-marker (nth 1 word))
                  (nth 2 word))))))

(defun sekken-word-end ()
  "入力中の語を終え、置き換えるべき語 (START END ROMAN) を返す。無ければ nil。
語を続けないコマンドが走る前に呼ぶ。語が無くても打ち始めを忘れ、
そのコマンドの後に打つ文字を新しい語にする。"
  (sekken-word--forget-pending)
  (prog1 (sekken-word--current)
    (sekken-word--forget-origin)))

(defun sekken-word--intact-p (start end roman)
  "START から END の語 ROMAN がそのまま残っているか。
候補の選択や undo で置き換わっていれば nil。"
  (and (>= start (point-min))
       (<= end (point-max))
       (equal (buffer-substring-no-properties start end) roman)))

(defun sekken-word--shrunk-p (start roman)
  "START から始まる語 ROMAN が、ポイントまでの先頭部分だけ残って縮んだか。
まとめて打った末尾を undo で消したときで、置き換わったのとは違い語は続く。"
  (and (>= start (point-min))
       (>= (point) start)
       (string-prefix-p (buffer-substring-no-properties start (point)) roman)))

(defun sekken-word--continuing-p (start end)
  "ポイントが START から END の語を続ける位置にあるか。
語の途中に戻ったのは離れたと見る。"
  (let ((bounds (sekken-word-bounds)))
    (and bounds
         (= (car bounds) start)
         (>= (point) end))))

(defun sekken-word-settle ()
  "覚えた語がコマンドの後どうなったかを見て、置き換えるべき語 (START END ROMAN) を返す。
置き換えないなら nil。語が置き換わっていれば重ねて確定はしないが、その語は
終わったので打ち始めを忘れる。その後ろで新しい語が始まっていれば、その打ち始めは
残す。縮んだだけなら語は続く。ポイントが語の末尾から離れていれば、その語を返して
終える。"
  (let (word)
    (when sekken-word--pending
      (let ((start (marker-position (nth 0 sekken-word--pending)))
            (end (marker-position (nth 1 sekken-word--pending)))
            (roman (nth 2 sekken-word--pending)))
        (cond
         ((not (sekken-word--intact-p start end roman))
          ;; 候補の選択が語を終えてから同じコマンドで文字を打つと、
          ;; 打ち始めは新しい語のもの。置き換わった語のものだけ忘れる。
          (unless (or (sekken-word--shrunk-p start roman)
                      (and sekken-word--origin
                           (> sekken-word--origin start)))
            (sekken-word--forget-origin)))
         ((sekken-word--continuing-p start end) nil)
         (t
          (setq word (list start end roman))
          ;; 境界だけの語のように置き換えられなくても、離れた語は終わり。
          (sekken-word--forget-origin)))))
    (sekken-word--forget-pending)
    word))

(defun sekken-word-reset ()
  "入力中の語の状態を捨てる。`sekken-mode' を切るときに呼ぶ。
覚えた語も捨てるので、切っている間に語が置き換わっても、次に入れたときの
後処理が古い語を確定しない。置き換えた語のローマ字の履歴は登録で使うので残す。"
  (sekken-word--forget-origin)
  (sekken-word--forget-finished)
  (sekken-word--forget-pending))

(provide 'sekken-word)
;;; sekken-word.el ends here
