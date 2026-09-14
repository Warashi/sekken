;;; sekken-input.el --- ポイント直前の入力の切り出しと表示 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 「入力中の語」は、自分が打ち始めた位置からポイントまでに連続する、
;; 空白以外の打てる文字（印字可能な ASCII）の並び。打ち始めは自己挿入の
;; コマンドが文字を入れた位置で、それより前には伸びない。もとからバッファにある文字や貼り
;; 付けた文字は打っていないので語にならず、確定の結果が英字で終わって
;; も、それをもう一度語として拾わない。確定を undo で取り消してローマ字
;; （かその先頭部分）が同じ位置に戻れば、打ち始めを復元して語に戻す。
;; 表示用には大文字境界を ▽ で示し、かなにして返す。エンジンには
;; 同じ分割をかなにし、区間の種類を付けた列として送る。
;;
;; 境界は大文字と `;' のほか、abbrev 区間を開く `/'、綴りをそのまま出す
;; literal 区間を開く `'', 接頭辞・接尾辞の印になる `>'。エンジン側
;; （sekken-rs/core/src/segment.rs）も同じ規則で分けるので、規則を変える
;; ときは両方を直す。

;;; Code:

(require 'cl-lib)
(require 'sekken-kana)
(require 'subr-x)

(defconst sekken-input--chars "!-~"
  "入力中の語を構成する文字（`skip-chars-backward' 用）。
空白を除く印字可能な ASCII、つまり `sekken-im' が拾う文字のうち空白以外。
数字や括弧も語の一部にして、1on1 のような語を 1 語として打てるようにする。")

(defcustom sekken-input-typing-commands
  '(sekken-im-self-insert self-insert-command)
  "文字を打つコマンド。これらが入れた文字だけを入力中の語にする。"
  :type '(repeat function)
  :group 'sekken)

(defvar-local sekken-input--origin nil
  "入力中の語の打ち始めの marker。無ければ語は無い。
marker は直前への挿入で進まないので、打ち始めの位置に打った文字は語に入る。")

(defun sekken-input-forget-origin ()
  "打ち始めを忘れる。次に打った文字が新しい語の打ち始めになる。"
  (when sekken-input--origin
    (set-marker sekken-input--origin nil)
    (setq sekken-input--origin nil)))

(defvar-local sekken-input--finished nil
  "最後に置き換えた語の (START . ROMAN)。START は marker。
undo が置き換えを取り消してローマ字が同じ位置に戻ったとき、語に戻すため。")

(defun sekken-input-finish-word (start roman)
  "START から始まっていた語 ROMAN を置き換え終えたことにする。
打ち始めを忘れ、次に打った文字は新しい語になる。"
  (sekken-input-forget-origin)
  (when sekken-input--finished
    (set-marker (car sekken-input--finished) nil))
  (setq sekken-input--finished (cons (copy-marker start) roman)))

(defun sekken-input-finished-roman ()
  "最後に置き換えた語のローマ字。無ければ nil。"
  (cdr sekken-input--finished))

(defun sekken-input-forget-finished ()
  "最後に置き換えた語を忘れる。"
  (when sekken-input--finished
    (set-marker (car sekken-input--finished) nil)
    (setq sekken-input--finished nil)))

(defvar-local sekken-input--restored nil
  "打つコマンド以外が、最後に置き換えた語の位置に戻した綴りの末尾。
redo は挿入を戻してもポイントを挿入位置の先頭に置くので、コマンドの後に
ポイントがそこに残っていれば、ここまで動かして語に戻す。")

(defun sekken-input--restored-p (beg end)
  "BEG から END への挿入が、最後に置き換えた語の位置に綴りの先頭部分を戻したか。"
  (and sekken-input--finished
       (marker-buffer (car sekken-input--finished))
       (= beg (marker-position (car sekken-input--finished)))
       (<= (- end beg) (length (cdr sekken-input--finished)))
       (string-prefix-p (buffer-substring-no-properties beg end)
                        (cdr sekken-input--finished))))

(defun sekken-input-revive-word ()
  "最後に置き換えた語のローマ字の先頭部分が同じ位置に戻り、ポイントがその中にあれば語に戻す。
undo が置き換えを取り消したときに、打ち始めからその語を続けられるようにする。
self-insert は打鍵を 20 文字ずつまとめて undo するので、戻るのは綴り全体とは
限らず先頭部分になる。redo のようにコマンドが綴りを戻してポイントをその先頭に
残していれば、末尾へ動かしてから語に戻す。打ち始めがあれば何もしない。"
  (let ((restored sekken-input--restored))
    (setq sekken-input--restored nil)
    (when (and sekken-input--finished
               (marker-buffer (car sekken-input--finished)))
      (let ((start (marker-position (car sekken-input--finished)))
            (roman (cdr sekken-input--finished)))
        ;; undo で語が空まで縮んだ後は打ち始めが残ったままなので、
        ;; ポイントを動かすのは打ち始めの有無によらない。
        (when (and restored (= (point) start))
          (goto-char restored))
        (when (and (null sekken-input--origin)
                   (> (point) start)
                   (<= (- (point) start) (length roman))
                   (string-prefix-p (buffer-substring-no-properties start (point)) roman))
          (setq sekken-input--origin (copy-marker start)))))))

(defun sekken-input-after-change (beg end old-length)
  "文字を打つコマンドが BEG に文字を入れたら、打ち始めが無ければそこを打ち始めにする。
`after-change-functions' 用。OLD-LENGTH が 0 でない置き換えと、
END が BEG の削除は打ち始めにしない。打つコマンド以外が最後に置き換えた語の
綴りを戻したなら、その末尾を覚える（`sekken-input-revive-word'）。"
  (when (and (zerop old-length)
             (< beg end))
    (if (memq this-command sekken-input-typing-commands)
        (unless sekken-input--origin
          (setq sekken-input--origin (copy-marker beg)))
      (when (sekken-input--restored-p beg end)
        (setq sekken-input--restored end)))))

(defun sekken-input-before-change (beg _end)
  "BEG から始まる編集が打ち始めより前に触れば打ち始めを忘れる。
`before-change-functions' 用。打ち始めへの挿入と、それより後ろの削除は
語の中の編集なので忘れない。削除は marker を動かすので、動く前に見る。"
  (when (and sekken-input--origin
             (< beg sekken-input--origin))
    (sekken-input-forget-origin)))

(defun sekken-input-bounds ()
  "ポイント直前の入力中の語の (START . END)。無ければ nil。
打ち始めより前には伸びない。"
  (when (and sekken-input--origin
             (< sekken-input--origin (point)))
    (let ((end (point))
          (start (save-excursion
                   (skip-chars-backward sekken-input--chars)
                   (max (point) sekken-input--origin))))
      (when (< start end)
        (cons start end)))))

(defun sekken-input-segment (roman)
  "ROMAN を変換境界で分け、(HEAD . SEGMENTS) を返す。
HEAD は先頭の境界より前の文字列。SEGMENTS の各要素は plist で、:text に
小文字化したローマ字（abbrev と literal なら綴りそのまま）、`/' で開いた
区間なら :abbrev t、`'' で開いた区間なら :literal t、直後に `>' があれば
:prefix t、直前に `>' があれば :suffix t を持つ。
大文字と単独のセミコロンが境界で、二重セミコロンはリテラルになる。
二重アポストロフィもどこでもリテラルの `'' になる（`'don''t'）。
境界で開いた直後の大文字や `;' は新しい区間を作らず、その区間を続ける。
開いた直後の `/' はその区間を abbrev に、`'' は literal にする。
abbrev と literal の区間は次の `;' `/' `'' まで続き、中の大文字は境界にしない。"
  (let ((head "")
        (segments nil)
        (current nil)
        ;; 境界で開いたばかりで、まだ文字の無い区間か。
        (fresh nil)
        ;; `/' か `'' で開いた、綴りのまま送る区間の中か。
        (in-spelled nil)
        (index 0))
    (cl-flet ((open (&rest props)
                (when current
                  (push current segments))
                (setq current (append (list :text "") props)))
              (append-text (string)
                (plist-put current :text (concat (plist-get current :text) string))))
      (while (< index (length roman))
        (let ((char (aref roman index)))
          (cond
           ((and (eq char ?')
                 (< (1+ index) (length roman))
                 (eq (aref roman (1+ index)) ?'))
            (if current
                (append-text "'")
              (setq head (concat head "'")))
            (setq fresh nil
                  index (+ index 2)))
           (in-spelled
            (if (memq char '(?\; ?/ ?'))
                (progn
                  (setq in-spelled nil)
                  (open)
                  (setq fresh t))
              (append-text (char-to-string char)))
            (setq index (1+ index)))
           ((and (<= ?A char) (<= char ?Z))
            (unless fresh
              (open))
            (append-text (char-to-string (+ char (- ?a ?A))))
            (setq fresh nil
                  index (1+ index)))
           ((and (eq char ?\;)
                 (< (1+ index) (length roman))
                 (eq (aref roman (1+ index)) ?\;))
            (if current
                (append-text ";")
              (setq head (concat head ";")))
            (setq fresh nil
                  index (+ index 2)))
           ((eq char ?\;)
            (unless fresh
              (open))
            (setq fresh t
                  index (1+ index)))
           ((memq char '(?/ ?'))
            ;; 綴りのまま送る区間は `>' の印を持たない。
            (unless fresh
              (open))
            (setq current (list :text "" (if (eq char ?/) :abbrev :literal) t)
                  in-spelled t
                  fresh nil
                  index (1+ index)))
           ((eq char ?>)
            (when current
              (plist-put current :prefix t))
            (open :suffix t)
            (setq fresh t
                  index (1+ index)))
           (t
            (if current
                (append-text (char-to-string char))
              (setq head (concat head (char-to-string char))))
            (setq fresh nil
                  index (1+ index))))))
      (when current
        (push current segments)))
    (cons head (nreverse segments))))

(defun sekken-input-has-boundary-p (roman)
  "ROMAN が変換境界（大文字・単独のセミコロン・`/'・`''・`>'）を含むか。"
  (and (cdr (sekken-input-segment roman)) t))

(defun sekken-input-converts-p (roman)
  "ROMAN に辞書を引く区間（convert か abbrev）があるか。
無ければエンジンに送っても文字列をつなぐだけなので、エディタで置き換えられる。"
  (and (cl-some (lambda (segment) (not (plist-get segment :literal)))
                (cdr (sekken-input-segment roman)))
       t))

(defun sekken-input-ready-p (roman)
  "ROMAN の最後の変換境界より後に入力があれば non-nil を返す。"
  (let ((segments (cdr (sekken-input-segment roman))))
    (and segments
         (not (string-empty-p (plist-get (car (last segments)) :text))))))

(defun sekken-input-literal (roman)
  "辞書を引く区間の無い ROMAN を、エンジンに送らずに出す文字列にする。
先頭の小文字はかなに、literal 区間は綴りのままつなぐ。"
  (let ((segmented (sekken-input-segment roman)))
    (concat (sekken-kana-roman-to-kana (car segmented))
            (mapconcat (lambda (segment) (plist-get segment :text))
                       (cdr segmented) ""))))

(defun sekken-input--segment-kana (segment lastp)
  "SEGMENT の :text をかなにする。
LASTP なら表示と同じく末尾の子音を変換せずに残す（`sekken-kana-display'）。
abbrev と literal の区間は綴りのまま返す。"
  (let ((text (plist-get segment :text)))
    (cond
     ((or (plist-get segment :abbrev) (plist-get segment :literal)) text)
     (lastp (sekken-kana-display text))
     (t (sekken-kana-roman-to-kana text)))))

(defun sekken-input-pieces (roman)
  "入力中の ROMAN をエンジンに送る区間の列にする。
先頭の小文字は kana、変換境界で始まる各部分は convert、`/' で開いた部分は
abbrev、`'' で開いた部分は literal の区間になり、`>' の印は :prefix と
:suffix で付ける。
jsonrpc.el が JSON の配列にするようベクタで返す。"
  (let* ((segmented (sekken-input-segment roman))
         (head (car segmented))
         (segments (cdr segmented))
         (pieces nil))
    (unless (string-empty-p head)
      (push (list :kind "kana"
                  :text (if segments
                            (sekken-kana-roman-to-kana head)
                          (sekken-kana-display head)))
            pieces))
    (while segments
      (let ((segment (car segments)))
        (push (append
               (list :kind (cond ((plist-get segment :abbrev) "abbrev")
                                 ((plist-get segment :literal) "literal")
                                 (t "convert"))
                     :text (sekken-input--segment-kana segment (null (cdr segments))))
               (and (plist-get segment :prefix) '(:prefix t))
               (and (plist-get segment :suffix) '(:suffix t)))
              pieces))
      (setq segments (cdr segments)))
    (vconcat (nreverse pieces))))

(defun sekken-input-display (roman)
  "入力中の ROMAN をかなにし、変換境界を ▽ で示す。
abbrev 区間は `/'、literal 区間は `'' を付けて綴りのまま、`>' は区間の後ろに示す。"
  (let* ((segmented (sekken-input-segment roman))
         (head (car segmented))
         (segments (cdr segmented))
         (display
          (if segments
              (sekken-kana-roman-to-kana head)
            (sekken-kana-display head))))
    (concat
     display
     (mapconcat
      (lambda (segment)
        (concat
         "▽"
         (and (plist-get segment :abbrev) "/")
         (and (plist-get segment :literal) "'")
         (sekken-input--segment-kana segment (eq segment (car (last segments))))
         (and (plist-get segment :prefix) ">")))
      segments ""))))

(provide 'sekken-input)
;;; sekken-input.el ends here
