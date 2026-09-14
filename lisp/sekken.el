;;; sekken.el --- SKK 風の一括変換による日本語入力 -*- lexical-binding: t -*-

;; Author: warashi
;; SPDX-License-Identifier: MIT
;; Version: 0.4.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: i18n, input method

;;; Commentary:
;; 漢字とかなの境界を大文字で示したローマ字列（WagahaihaNekodearu.）を
;; そのまま打つと、打鍵に合わせて「吾輩は猫である。」が overlay に見え、
;; 語の終わり（空白や改行、送信や保存）で確定の打鍵なしに置き換わる。
;; 1 位以外の候補は `sekken-convert' で選ぶ。入力モードの切り替えや
;; 未確定状態を持たない。
;;
;; `C-\' で input method として有効にする。候補が届くまでは入力中の語を
;; かなで見せ、大文字またはセミコロンの境界を ▽ で示す。英単語は `''
;; で開いて綴りのまま出す。

;;; Code:

(require 'sekken-server)
(require 'sekken-overlay)
(require 'sekken-convert)
(require 'sekken-live)
(require 'sekken-register)
(require 'sekken-im)

(defcustom sekken-convert-key "C-j"
  "`sekken-convert' を割り当てるキー。"
  :type 'key-sequence
  :group 'sekken)

(defvar sekken-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd sekken-convert-key) #'sekken-convert)
    (sekken-im-bind-map map)
    map)
  "`sekken-mode' のキーマップ。")

;;;###autoload
(define-minor-mode sekken-mode
  "SKK 風の一括変換による日本語入力。"
  :lighter " 石"
  :keymap sekken-mode-map
  (if sekken-mode
      (progn
        (add-hook 'pre-command-hook #'sekken-live-before-command nil t)
        (add-hook 'post-command-hook #'sekken-live-after-command nil t)
        (add-hook 'before-change-functions #'sekken-input-before-change nil t)
        (add-hook 'after-change-functions #'sekken-input-after-change nil t))
    (remove-hook 'pre-command-hook #'sekken-live-before-command t)
    (remove-hook 'post-command-hook #'sekken-live-after-command t)
    (remove-hook 'before-change-functions #'sekken-input-before-change t)
    (remove-hook 'after-change-functions #'sekken-input-after-change t)
    (sekken-input-forget-origin)
    (sekken-input-forget-finished)
    (sekken-overlay-clear)
    (when (and (equal current-input-method sekken-im-name)
               (not sekken-im--deactivating))
      (deactivate-input-method))))

(register-input-method
 sekken-im-name "Japanese" #'sekken-im-activate "-かな:-"
 "SKK 風の境界を使う一括変換日本語入力。")

(provide 'sekken)
;;; sekken.el ends here
