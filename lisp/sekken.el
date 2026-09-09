;;; sekken.el --- SKK 風の一括変換による日本語入力 -*- lexical-binding: t -*-

;; Author: warashi
;; SPDX-License-Identifier: MIT
;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: i18n, input method

;;; Commentary:
;; 漢字とかなの境界を大文字で示したローマ字列（WagahaihaNekodearu.）を
;; そのまま打ち、`sekken-convert' 1 つで「吾輩は猫である。」に置き換える。
;; 入力モードの切り替えや未確定状態を持たない。
;;
;; `sekken-mode' を有効にしたバッファでだけ働く。入力中の語は overlay で
;; かなに見せ、大文字境界を ▽ で示す。

;;; Code:

(require 'sekken-server)
(require 'sekken-overlay)
(require 'sekken-convert)

(defcustom sekken-convert-key "C-j"
  "`sekken-convert' を割り当てるキー。"
  :type 'key-sequence
  :group 'sekken)

(defvar sekken-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd sekken-convert-key) #'sekken-convert)
    map)
  "`sekken-mode' のキーマップ。")

;;;###autoload
(define-minor-mode sekken-mode
  "SKK 風の一括変換による日本語入力。"
  :lighter " 石"
  :keymap sekken-mode-map
  (if sekken-mode
      (progn
        (add-hook 'post-command-hook #'sekken-overlay-update nil t)
        (add-hook 'completion-at-point-functions #'sekken-completion-at-point nil t))
    (remove-hook 'post-command-hook #'sekken-overlay-update t)
    (remove-hook 'completion-at-point-functions #'sekken-completion-at-point t)
    (sekken-overlay-clear)))

(provide 'sekken)
;;; sekken.el ends here
