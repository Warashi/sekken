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
  "投機的変換で候補を採点する文字言語モデルファイル（train-lm の出力）。
エンジンはこれが無いと起動しないので、設定しないと変換できない。"
  :type '(choice (const nil) file))

(defcustom sekken-server-user-jisyo (locate-user-emacs-file "sekken-jisyo")
  "ユーザー辞書ファイル。`sekken-register' で登録した語をエンジンが書く。
最初の登録まで無くてよい。nil なら登録できない。"
  :type '(choice (const nil) file))

(defcustom sekken-server-adapted-lm (locate-user-emacs-file "sekken-lm.zst")
  "確定した文で動かした言語モデルの保存先。
エンジンは起動時にこのファイルがあれば `sekken-server-lm' より先に読み、
終了時に動かした分を書く。nil なら個人化しない。"
  :type '(choice (const nil) file))

(defcustom sekken-server-timeout 5
  "変換要求の応答を待つ秒数。"
  :type 'number)

(defcustom sekken-server-startup-timeout 60
  "起動後、初回の変換が成功するまでの間、変換要求の応答を待つ秒数。
エンジンはモデルの読み込みを終えるまで変換に応答しない。"
  :type 'number)

(defconst sekken-server-version "0.5.0"
  "この Lisp が想定するエンジンのバージョン。")

(defconst sekken-server--max-crashes 3
  "自動再起動をやめるまでの連続異常終了回数。")

(defconst sekken-server--load-failed-code -32000
  "エンジンがモデルの読み込みに失敗したときの JSON-RPC エラーコード。
エンジンはこの応答を書いた直後に終了するが、Emacs 側が応答を処理する
時点ではまだプロセスが生きて見えることがあるので、コードで判定する。")

(defvar sekken-server--connection nil
  "エンジンとの `jsonrpc-process-connection'。")

(defvar sekken-server--warmed nil
  "現在の接続で変換が一度でも成功していれば non-nil。
nil の間はモデルの読み込み中とみなし、`sekken-server-startup-timeout' で待つ。")

(defvar sekken-server--crashes 0
  "直近の連続異常終了回数。正常応答で 0 に戻す。")

(defun sekken-server--command ()
  "エンジンを起動するコマンドライン。設定が欠けていればエラーにする。"
  (dolist (pair `((sekken-server-dic . ,sekken-server-dic)
                  (sekken-server-model . ,sekken-server-model)
                  (sekken-server-jisyo . ,sekken-server-jisyo)
                  (sekken-server-lm . ,sekken-server-lm)))
    (unless (cdr pair)
      (user-error "sekken: %s が設定されていません" (car pair))))
  (append
   (list sekken-server-program "server"
         "--dic" (expand-file-name sekken-server-dic)
         "--model" (expand-file-name sekken-server-model)
         "--jisyo" (expand-file-name sekken-server-jisyo)
         "--lm" (expand-file-name sekken-server-lm))
   (when sekken-server-user-jisyo
     (list "--user-jisyo" (expand-file-name sekken-server-user-jisyo)))
   (when sekken-server-adapted-lm
     (list "--adapted-lm" (expand-file-name sekken-server-adapted-lm)))))

(defun sekken-server--make-process ()
  "エンジンのプロセスを作る。"
  (make-process :name "sekken"
                :command (sekken-server--command)
                :connection-type 'pipe
                :coding 'utf-8-unix
                :noquery t
                ;; jsonrpc.el は "*NAME stderr*" という名前のバッファを自分で作り、
                ;; プロセス生成後に隠し名へ rename する。別名を渡すと jsonrpc.el が
                ;; 隠し名の既存バッファを kill し、stderr のパイプが閉じる。
                :stderr (get-buffer-create "*sekken stderr*")))

(defun sekken-server--on-shutdown (conn)
  "接続が閉じたときの後始末。"
  (when (eq conn sekken-server--connection)
    (setq sekken-server--connection nil)))

(defun sekken-server--connect ()
  "接続を作り、バージョンを照合して返す。
エンジンはモデルの読み込み中でも version に即応答する。"
  (let ((conn (make-instance 'jsonrpc-process-connection
                             :name "sekken"
                             :process #'sekken-server--make-process
                             :on-shutdown #'sekken-server--on-shutdown))
        connected)
    (setq sekken-server--connection conn
          sekken-server--warmed nil)
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

(defun sekken-server--henkan-timeout ()
  "変換要求の応答を待つ秒数。初回の変換が成功するまでは起動中として長く待つ。"
  (if sekken-server--warmed
      sekken-server-timeout
    sekken-server-startup-timeout))

(defun sekken-server--crash-code-p (code)
  "JSON-RPC エラーコード CODE の失敗をエンジンの異常終了として数えるなら non-nil。"
  (or (not (and sekken-server--connection
                (jsonrpc-running-p sekken-server--connection)))
      (eql code sekken-server--load-failed-code)))

(defun sekken-server--crash-p (err)
  "変換要求の失敗 ERR をエンジンの異常終了として数えるなら non-nil。"
  (sekken-server--crash-code-p (alist-get 'jsonrpc-error-code (cdr err))))

(defun sekken-server--count-crash (code)
  "エラーコード CODE の失敗が異常終了なら連続異常終了として数える。"
  (when (sekken-server--crash-code-p code)
    (setq sekken-server--crashes (1+ sekken-server--crashes))))

(defun sekken-server--succeeded (result)
  "変換の応答 RESULT を候補文字列のリストにし、正常応答として記録する。"
  (setq sekken-server--crashes 0
        sekken-server--warmed t)
  (append (plist-get result :candidates) nil))

(defun sekken-server-henkan (pieces top)
  "PIECES を変換し、候補文字列のリストを最大 TOP 個返す。
PIECES は `sekken-input-pieces' が返す、種類付きのかな区間のベクタ。"
  (condition-case err
      (sekken-server--succeeded
       (jsonrpc-request
        (sekken-server-connection) :henkan
        (list :pieces pieces :top top)
        :timeout (sekken-server--henkan-timeout)))
    (error
     ;; エンジンの異常終了もタイムアウトも jsonrpc-error で届くので、
     ;; エラーの種類ではなくプロセスが死んだかどうかで数える。
     (when (sekken-server--crash-p err)
       (setq sekken-server--crashes (1+ sekken-server--crashes)))
     (signal (car err) (cdr err)))))

(defun sekken-server-henkan-async (pieces top on-success &optional on-failure)
  "PIECES を非同期に変換し、候補文字列のリスト（最大 TOP 個）を ON-SUCCESS に渡す。
失敗とタイムアウトは `sekken-server-henkan' と同じく異常終了として数え、
ON-FAILURE があれば引数なしで呼ぶ。接続の起動に失敗すればその場でエラーを投げる。"
  (jsonrpc-async-request
   (sekken-server-connection) :henkan
   (list :pieces pieces :top top)
   :timeout (sekken-server--henkan-timeout)
   :success-fn (lambda (result)
                 (funcall on-success (sekken-server--succeeded result)))
   :error-fn (lambda (err)
               (sekken-server--count-crash (plist-get err :code))
               (when on-failure (funcall on-failure)))
   :timeout-fn (lambda ()
                 (sekken-server--count-crash nil)
                 (when on-failure (funcall on-failure)))))

(defvar sekken-server--adapt-error nil
  "直近の学習要求が失敗していればその理由の文字列。成功すれば nil に戻す。")

(defun sekken-server--adapt-failed (code message)
  "学習要求の失敗を記録して知らせる。CODE と MESSAGE はエンジンの応答。"
  (setq sekken-server--adapt-error (or message "timeout"))
  (sekken-server--count-crash code)
  (message "sekken: 確定した文を学習できませんでした: %s"
           sekken-server--adapt-error))

(defun sekken-server-adapt (pieces sentence)
  "PIECES を変換して確定した文 SENTENCE をエンジンに学習させる。
非同期に送り、応答は待たない。学習のためだけにエンジンを起動はしないので、
動いている接続が無ければ何もしない。失敗は `sekken-server--adapt-error' に
残して知らせ、変換と同じく異常終了として数える。送ること自体の失敗も同じで、
確定の hook から呼ばれるので、エラーを投げて hook を外させない。"
  (when (and sekken-server--connection
             (jsonrpc-running-p sekken-server--connection))
    (condition-case err
        (jsonrpc-async-request
         sekken-server--connection :adapt
         (list :pieces pieces :sentence sentence)
         :timeout (sekken-server--henkan-timeout)
         :success-fn (lambda (_result) (setq sekken-server--adapt-error nil))
         :error-fn (lambda (err)
                     (sekken-server--adapt-failed (plist-get err :code)
                                                  (plist-get err :message)))
         :timeout-fn (lambda () (sekken-server--adapt-failed nil nil)))
      (error (sekken-server--adapt-failed nil (error-message-string err))))))

(defun sekken-server-register (yomi surface)
  "送りなしの読み YOMI の語 SURFACE をユーザー辞書に登録する。
エンジンがメモリに足してファイルに書く。断られれば `user-error' にする。"
  (condition-case err
      (jsonrpc-request (sekken-server-connection) :register
                       (list :yomi yomi :surface surface)
                       :timeout (sekken-server--henkan-timeout))
    (jsonrpc-error
     (user-error "sekken: 登録できませんでした: %s"
                 (or (alist-get 'jsonrpc-error-message (cdr err))
                     (error-message-string err))))))

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
