;;; sekken.el --- SKK 風の一括変換による日本語入力 -*- lexical-binding: t -*-

;; Author: warashi
;; SPDX-License-Identifier: MIT
;; Version: 0.3.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: i18n, input method

;;; Commentary:
;; 漢字とかなの境界を大文字で示したローマ字列（WagahaihaNekodearu.）を
;; そのまま打つと、打鍵に合わせて「吾輩は猫である。」が overlay に見え、
;; 語の終わり（空白や改行）で確定の打鍵なしに置き換わる。1 位以外の
;; 候補は `sekken-convert' で選ぶ。入力モードの切り替えや未確定状態を
;; 持たない。
;;
;; `C-\' で input method として有効にする。候補が届くまでは入力中の語を
;; かなで見せ、大文字またはセミコロンの境界を ▽ で示す。英単語は `''
;; で開いて綴りのまま出す。

;;; Code:

(require 'sekken-server)
(require 'sekken-overlay)
(require 'sekken-convert)
(require 'sekken-live)
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
        (add-hook 'post-command-hook #'sekken-live-after-command nil t))
    (remove-hook 'pre-command-hook #'sekken-live-before-command t)
    (remove-hook 'post-command-hook #'sekken-live-after-command t)
    (sekken-overlay-clear)
    (when (and (equal current-input-method sekken-im-name)
               (not sekken-im--deactivating))
      (deactivate-input-method))))

(register-input-method
 sekken-im-name "Japanese" #'sekken-im-activate "-かな:-"
 "SKK 風の境界を使う一括変換日本語入力。")

(provide 'sekken)
;;; sekken.el ends here
