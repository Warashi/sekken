;;; e2e.el --- 実際のエンジンを起動して通しで確かめる -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; ERT の単体テストはエンジン呼び出しを差し替えている。ここでは本物の
;; `sekken server' を起動し、JSON-RPC の枠組み・overlay・終了処理まで通す。
;; 環境変数 SEKKEN_BIN / SEKKEN_DIC / SEKKEN_MODEL / SEKKEN_JISYO が
;; 揃っていなければ何もせず終わる。
;;
;;   SEKKEN_BIN=... SEKKEN_DIC=... SEKKEN_MODEL=... SEKKEN_JISYO=... \
;;     emacs -Q --batch -L lisp -l lisp/test/e2e.el

;;; Code:

(require 'sekken)

(let ((bin (getenv "SEKKEN_BIN"))
      (dic (getenv "SEKKEN_DIC"))
      (model (getenv "SEKKEN_MODEL"))
      (jisyo (getenv "SEKKEN_JISYO")))
  (if (not (and bin dic model jisyo))
      (message "e2e: SEKKEN_BIN / SEKKEN_DIC / SEKKEN_MODEL / SEKKEN_JISYO が無いので skip")
    (setq sekken-server-program bin
          sekken-server-dic dic
          sekken-server-model model
          sekken-server-jisyo jisyo)
    (let ((henkan (sekken-server-henkan
                   (sekken-input-pieces "WagahaihaNekodearu.") 3)))
      (message "henkan: %S" henkan)
      (unless (member "吾輩は猫である。" henkan)
        (error "e2e: 変換候補に期待する文が無い")))
    (with-temp-buffer
      (activate-input-method "japanese-sekken")
      (unless (and sekken-mode
                   (equal current-input-method-title "-かな:-"))
        (error "e2e: input method が有効になっていない"))
      (insert "kyouha")
      (sekken-convert)
      (unless (equal (buffer-string) "きょうは")
        (error "e2e: ひらがな置換が %S" (buffer-string)))
      (insert " IiTenkidesune.")
      (sekken-live-update)
      (let ((display (overlay-get sekken-overlay--overlay 'display)))
        (unless (equal display "▽いい▽てんきですね。")
          (error "e2e: 候補が届く前の overlay が %S" display)))
      ;; 先読みの返事を待つと overlay が 1 位候補になる。
      (with-timeout (10 (error "e2e: 先読みの返事が来ない"))
        (while (not (sekken-convert-cached "IiTenkidesune."))
          (accept-process-output nil 0.1)))
      (let ((display (overlay-get sekken-overlay--overlay 'display)))
        (unless (equal display (car (sekken-convert-cached "IiTenkidesune.")))
          (error "e2e: 候補が届いた後の overlay が %S" display)))
      (let ((completion-in-region-function
             (lambda (start end table &optional _pred)
               (let ((cands (all-completions (buffer-substring start end) table)))
                 (delete-region start end)
                 (insert (car cands))))))
        (sekken-convert))
      (message "converted: %S" (buffer-string))
      (unless (string-prefix-p "きょうは " (buffer-string))
        (error "e2e: 変換結果が %S" (buffer-string)))
      (erase-buffer)
      (insert ";wagahai;ha;neko;dearu.")
      (let ((completion-in-region-function
             (lambda (start end table &optional _pred)
               (let ((cands (all-completions (buffer-substring start end) table)))
                 (delete-region start end)
                 (insert (car cands))))))
        (sekken-convert))
      (unless (member (buffer-string)
                      '("我輩は猫である。" "吾輩は猫である。"))
        (error "e2e: sticky 変換結果が %S" (buffer-string)))
      (deactivate-input-method))
    (let ((proc (jsonrpc--process (sekken-server-connection))))
      (sekken-server-shutdown)
      (when (process-live-p proc)
        (error "e2e: shutdown 後もエンジンが生きている")))
    (message "e2e: ok")))

;;; e2e.el ends here
