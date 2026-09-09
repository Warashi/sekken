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

(defconst sekken-server-version "0.1.0"
  "この Lisp が想定するエンジンのバージョン。")

(defconst sekken-server--max-crashes 3
  "自動再起動をやめるまでの連続異常終了回数。")

(defvar sekken-server--connection nil
  "エンジンとの `jsonrpc-process-connection'。")

(defvar sekken-server--crashes 0
  "直近の連続異常終了回数。正常応答で 0 に戻す。")

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

(defun sekken-server--on-shutdown (_conn)
  "接続が閉じたときの後始末。"
  (setq sekken-server--connection nil))

(defun sekken-server--connect ()
  "接続を作り、バージョンを照合して返す。"
  (let ((conn (make-instance 'jsonrpc-process-connection
                             :name "sekken"
                             :process #'sekken-server--make-process
                             :on-shutdown #'sekken-server--on-shutdown)))
    (let ((version (plist-get (jsonrpc-request conn :version nil
                                               :timeout sekken-server-timeout)
                              :version)))
      (unless (equal version sekken-server-version)
        (display-warning 'sekken
                         (format "エンジンのバージョン %s は想定 %s と異なります"
                                 version sekken-server-version))))
    conn))

(defun sekken-server-connection ()
  "動いている接続を返す。無ければ起動する。"
  (when (and sekken-server--connection
             (not (jsonrpc-running-p sekken-server--connection)))
    (setq sekken-server--connection nil))
  (or sekken-server--connection
      (progn
        (when (>= sekken-server--crashes sekken-server--max-crashes)
          (user-error "sekken: エンジンが連続して落ちました。M-x sekken-server-restart で再起動してください"))
        (setq sekken-server--connection (sekken-server--connect)))))

(defun sekken-server-henkan (input top)
  "INPUT を変換し、候補文字列のリストを最大 TOP 個返す。"
  (condition-case err
      (let ((result (jsonrpc-request (sekken-server-connection) :henkan
                                     (list :input input :top top)
                                     :timeout sekken-server-timeout)))
        (setq sekken-server--crashes 0)
        (append (plist-get result :candidates) nil))
    (error
     ;; エンジンの異常終了もタイムアウトも jsonrpc-error で届くので、
     ;; エラーの種類ではなくプロセスが死んだかどうかで数える。
     (unless (and sekken-server--connection
                  (jsonrpc-running-p sekken-server--connection))
       (setq sekken-server--crashes (1+ sekken-server--crashes)))
     (signal (car err) (cdr err)))))

(defun sekken-server-shutdown ()
  "エンジンを止める。"
  (interactive)
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

(provide 'sekken-server)
;;; sekken-server.el ends here
