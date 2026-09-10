;;; sekken-im.el --- Emacs input method integration for sekken -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; Printable ASCII input is translated to private events before keymap lookup.
;; This lets sekken receive text even when a major mode binds letters directly.

;;; Code:

(defconst sekken-im-name "japanese-sekken"
  "Name registered in `input-method-alist'.")

(defvar sekken-im--deactivating nil)
(defvar corfu-auto-commands)
(defvar sekken-mode)

(declare-function sekken-mode "sekken" (&optional arg))
(declare-function sekken-server-schedule-prewarm "sekken-server" ())

(defun sekken-im--event (char)
  "Return the private input event for ASCII character CHAR."
  (let ((event (intern (format "sekken-key-%d" char))))
    (put event 'sekken-im-char char)
    event))

(defun sekken-im-bind-map (map)
  "Bind sekken private input events in MAP."
  (dotimes (offset 95)
    (define-key map
                (vector (sekken-im--event (+ 32 offset)))
                #'sekken-im-self-insert))
  map)

(defun sekken-im--pass-through-p (key)
  "Return non-nil when KEY must remain available to Emacs keymaps."
  (or (and (or buffer-read-only
               (and (get-char-property (point) 'read-only)
                    (get-char-property (point) 'front-sticky)))
           (not (or inhibit-read-only
                    (get-char-property (point) 'inhibit-read-only))))
      (and overriding-terminal-local-map
           (lookup-key overriding-terminal-local-map (vector key)))
      overriding-local-map))

(defun sekken-im-filter (key)
  "Translate printable ASCII KEY to a sekken private event."
  (if (or (not sekken-mode)
          (< key 32)
          (> key 126)
          (sekken-im--pass-through-p key))
      (list key)
    (list (sekken-im--event key))))

(defun sekken-im-self-insert (count)
  "Insert the character carried by the current private event COUNT times."
  (interactive "p")
  (let ((char (get last-command-event 'sekken-im-char)))
    (unless char
      (user-error "sekken: 入力文字を持たないイベントです"))
    (let ((last-command-event char))
      (self-insert-command count))))

(put 'sekken-im-self-insert 'delete-selection
     'delete-selection-uses-region-p)

(defun sekken-im-setup-corfu ()
  "Tell corfu that sekken's insertion command can trigger auto completion."
  (when (boundp 'corfu-auto-commands)
    (add-to-list 'corfu-auto-commands 'sekken-im-self-insert)))

(defun sekken-im-activate (_input-method)
  "Activate sekken for the current buffer."
  (setq-local input-method-function #'sekken-im-filter)
  (setq-local deactivate-current-input-method-function #'sekken-im-deactivate)
  (sekken-im-setup-corfu)
  (sekken-mode 1)
  (sekken-server-schedule-prewarm))

(defun sekken-im-deactivate ()
  "Deactivate sekken for the current buffer."
  (let ((sekken-im--deactivating t))
    (sekken-mode -1)))

(defun sekken-im-after-change-major-mode ()
  "Restore sekken's buffer-local mode after changing major mode."
  (when (equal current-input-method sekken-im-name)
    (sekken-mode 1)))

(with-eval-after-load 'corfu
  (sekken-im-setup-corfu))
(with-eval-after-load 'corfu-auto
  (sekken-im-setup-corfu))

(add-hook 'after-change-major-mode-hook #'sekken-im-after-change-major-mode)

(provide 'sekken-im)
;;; sekken-im.el ends here
