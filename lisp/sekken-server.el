;;; sekken-server.el --- 変換エンジンの常駐プロセス管理 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken server' を Emacs 全体で 1 つ起動し、jsonrpc.el で stdio 上の
;; JSON-RPC を張る。初回の変換で遅延起動し、異常終了は次回の変換で
;; 再起動する。連続して落ちたら `sekken-server-restart' を待つ。

;;; Code:

(require 'jsonrpc)

(defgroup sekken nil
  "SKK 風の一括変換による日本語入力。"
  :group 'i18n
  :prefix "sekken-")

(defcustom sekken-server-program "sekken"
  "変換エンジンの実行ファイル。"
  :type 'string)

(defcustom sekken-server-dic nil
  "vibrato の辞書ファイル（system.dic.zst）。"
  :type '(choice (const nil) file))

(defcustom sekken-server-model nil
  "学習済み n-gram モデルファイル。"
  :type '(choice (const nil) file))

(defcustom sekken-server-jisyo nil
  "SKK 辞書ファイル（SKK-JISYO.L など）。"
  :type '(choice (const nil) file))

(defcustom sekken-server-lm nil
  "N-best を並べ替える文字言語モデルファイル（train-lm の出力）。nil なら並べ替えない。"
  :type '(choice (const nil) file))

(defcustom sekken-server-timeout 5
  "変換要求の応答を待つ秒数。"
  :type 'number)

(defcustom sekken-server-prewarm-delay 1
  "起動後に変換エンジンを先読みするまでのアイドル秒数。"
  :type 'number)

(defconst sekken-server-version "0.1.0"
  "この Lisp が想定するエンジンのバージョン。")

(defconst sekken-server--max-crashes 3
  "自動再起動をやめるまでの連続異常終了回数。")

(defconst sekken-server--input-canceled 'sekken-server-input-canceled
  "打鍵によって JSON-RPC 要求を中断したことを表す値。")

(defvar sekken-server--connection nil
  "エンジンとの `jsonrpc-process-connection'。")

(defvar sekken-server--crashes 0
  "直近の連続異常終了回数。正常応答で 0 に戻す。")

(defvar sekken-server--prewarm-timer nil
  "変換エンジンを先読みする idle timer。")

(defun sekken-server--command ()
  "エンジンを起動するコマンドライン。設定が欠けていればエラーにする。"
  (dolist (pair `((sekken-server-dic . ,sekken-server-dic)
                  (sekken-server-model . ,sekken-server-model)
                  (sekken-server-jisyo . ,sekken-server-jisyo)))
    (unless (cdr pair)
      (user-error "sekken: %s が設定されていません" (car pair))))
  (append
   (list sekken-server-program "server"
         "--dic" (expand-file-name sekken-server-dic)
         "--model" (expand-file-name sekken-server-model)
         "--jisyo" (expand-file-name sekken-server-jisyo))
   (when sekken-server-lm
     (list "--lm" (expand-file-name sekken-server-lm)))))

(defun sekken-server--make-process ()
  "エンジンのプロセスを作る。"
  (make-process :name "sekken"
                :command (sekken-server--command)
                :connection-type 'pipe
                :coding 'utf-8-unix
                :noquery t
                :stderr (get-buffer-create " *sekken stderr*")))

(defun sekken-server--on-shutdown (conn)
  "接続が閉じたときの後始末。"
  (when (eq conn sekken-server--connection)
    (setq sekken-server--connection nil)))

(defun sekken-server--connect ()
  "接続を作り、バージョンを照合して返す。
エンジンはモデルの読み込み中でも version に即応答するので、打鍵で
取り消さない。取り消すと読み込み中のプロセスを kill してしまう。"
  (let ((conn (make-instance 'jsonrpc-process-connection
                             :name "sekken"
                             :process #'sekken-server--make-process
                             :on-shutdown #'sekken-server--on-shutdown))
        connected)
    (setq sekken-server--connection conn)
    (unwind-protect
        (let ((version (plist-get (jsonrpc-request
                                   conn :version nil
                                   :timeout sekken-server-timeout)
                                  :version)))
          (unless (equal version sekken-server-version)
            (display-warning
             'sekken
             (format "エンジンのバージョン %s は想定 %s と異なります"
                     version sekken-server-version)))
          (setq connected t)
          conn)
      (unless connected
        (when (eq conn sekken-server--connection)
          (setq sekken-server--connection nil))
        (ignore-errors (jsonrpc-shutdown conn))))))

(defun sekken-server-connection ()
  "動いている接続を返す。無ければ起動する。"
  (when (and sekken-server--connection
             (not (jsonrpc-running-p sekken-server--connection)))
    (setq sekken-server--connection nil))
  (or sekken-server--connection
      (progn
        (when (>= sekken-server--crashes sekken-server--max-crashes)
          (user-error "sekken: エンジンが連続して落ちました。M-x sekken-server-restart で再起動してください"))
        (sekken-server--connect))))

(defun sekken-server-henkan (input top &optional cancel-on-input)
  "INPUT を変換し、候補文字列のリストを最大 TOP 個返す。"
  (condition-case err
      (let ((result
             (jsonrpc-request
              (sekken-server-connection) :henkan
              (list :input input :top top)
              :timeout sekken-server-timeout
              :cancel-on-input cancel-on-input
              :cancel-on-input-retval sekken-server--input-canceled)))
        (setq sekken-server--crashes 0)
        (if (eq result sekken-server--input-canceled)
            result
          (append (plist-get result :candidates) nil)))
    (error
     ;; エンジンの異常終了もタイムアウトも jsonrpc-error で届くので、
     ;; エラーの種類ではなくプロセスが死んだかどうかで数える。
     (unless (and sekken-server--connection
                  (jsonrpc-running-p sekken-server--connection))
       (setq sekken-server--crashes (1+ sekken-server--crashes)))
     (signal (car err) (cdr err)))))

(defun sekken-server--configured-p ()
  "必須のエンジン設定がすべて揃っていれば non-nil を返す。"
  (and sekken-server-program
       sekken-server-dic
       sekken-server-model
       sekken-server-jisyo))

(defun sekken-server--prewarm-attempt ()
  "入力が無い間に本番と同じ変換要求まで通す。
打鍵によって中断された場合は nil、完了した場合は t を返す。"
  (condition-case nil
      (not (eq (sekken-server-henkan "Kana" 1 t)
               sekken-server--input-canceled))
    (quit nil)))

(defun sekken-server--run-prewarm ()
  "予約された先読みを実行し、中断された場合は予約し直す。"
  (setq sekken-server--prewarm-timer nil)
  (when (sekken-server--configured-p)
    (unless (sekken-server--prewarm-attempt)
      (sekken-server-schedule-prewarm))))

(defun sekken-server-schedule-prewarm ()
  "変換エンジンの先読みを予約する。
設定値は timer の実行時に確認するため、設定より前に呼んでもよい。"
  (when (and (not sekken-server--prewarm-timer)
             (not (and sekken-server--connection
                       (jsonrpc-running-p sekken-server--connection))))
    (setq sekken-server--prewarm-timer
          (run-with-idle-timer sekken-server-prewarm-delay nil
                               #'sekken-server--run-prewarm))))

(defun sekken-server-shutdown ()
  "エンジンを止める。"
  (interactive)
  (when sekken-server--prewarm-timer
    (cancel-timer sekken-server--prewarm-timer)
    (setq sekken-server--prewarm-timer nil))
  (when sekken-server--connection
    (let ((conn sekken-server--connection))
      (setq sekken-server--connection nil)
      ;; `jsonrpc-shutdown' は何も送らずプロセスの終了を待つだけなので、
      ;; LSP と同じく shutdown 要求と exit 通知を自分で送る。
      (ignore-errors (jsonrpc-request conn :shutdown nil :timeout 1))
      (ignore-errors (jsonrpc-notify conn :exit nil))
      (ignore-errors (jsonrpc-shutdown conn)))))

(defun sekken-server-restart ()
  "エンジンを再起動する。"
  (interactive)
  (sekken-server-shutdown)
  (setq sekken-server--crashes 0)
  (sekken-server-connection))

(add-hook 'kill-emacs-hook #'sekken-server-shutdown)
(if after-init-time
    (sekken-server-schedule-prewarm)
  (add-hook 'emacs-startup-hook #'sekken-server-schedule-prewarm))

(provide 'sekken-server)
;;; sekken-server.el ends here
