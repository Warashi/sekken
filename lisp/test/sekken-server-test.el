;;; sekken-server-test.el --- sekken-server のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'sekken-server)

(ert-deftest sekken-server/設定からコマンドラインを組み立てる ()
  (let ((sekken-server-program "/opt/sekken")
        (sekken-server-dic "/d/system.dic.zst")
        (sekken-server-model "/d/model.zst")
        (sekken-server-jisyo "/d/SKK-JISYO.L"))
    (should (equal (sekken-server--command)
                   '("/opt/sekken" "server"
                     "--dic" "/d/system.dic.zst"
                     "--model" "/d/model.zst"
                     "--jisyo" "/d/SKK-JISYO.L")))))

(ert-deftest sekken-server/言語モデルを設定すれば--lmを付ける ()
  (let ((sekken-server-program "/opt/sekken")
        (sekken-server-dic "/d/system.dic.zst")
        (sekken-server-model "/d/model.zst")
        (sekken-server-jisyo "/d/SKK-JISYO.L")
        (sekken-server-lm "/d/lm.zst"))
    (should (equal (sekken-server--command)
                   '("/opt/sekken" "server"
                     "--dic" "/d/system.dic.zst"
                     "--model" "/d/model.zst"
                     "--jisyo" "/d/SKK-JISYO.L"
                     "--lm" "/d/lm.zst")))))

(ert-deftest sekken-server/設定が欠けていればエラーにする ()
  (let ((sekken-server-dic nil))
    (should-error (sekken-server--command) :type 'user-error)))

(ert-deftest sekken-server/プロセスが死んで失敗したら連続異常終了として数える ()
  (require 'cl-lib)
  (let ((sekken-server--connection nil)
        (sekken-server--crashes 0))
    (cl-letf (((symbol-function 'sekken-server-connection)
               (lambda () (setq sekken-server--connection nil) 'dead))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _) (signal 'jsonrpc-error '("dead")))))
      (should-error (sekken-server-henkan "Neko" 3))
      (should (= sekken-server--crashes 1)))))

(provide 'sekken-server-test)
;;; sekken-server-test.el ends here
