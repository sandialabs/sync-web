;; Exercise help/documentation/signature over every rootlet binding.
;; Return compact counts rather than the full help text corpus.

(define denied-authority-names
  '(*rootlet-redefinition-hook* *read-error-hook* *error-hook* *autoload-hook*
    *load-hook* *missing-close-paren-hook* *unbound-variable-hook*
    hook-functions make-hook require *autoload* *cload-directory* *load-path*
    profile-in abort exit emergency-exit gc stacktrace autoload load
    open-input-file open-output-file call-with-input-file call-with-output-file
    with-input-from-file with-output-to-file port-file port-filename
    c-pointer->list c-pointer-weak2 c-pointer-weak1 c-pointer-type
    c-pointer-info c-pointer c-object-type c-object? c-pointer?
    random-state->list random-state random random-state?))

(define (denied-authority-name? sym)
  (memq sym denied-authority-names))

(define rootlet-symbols
  (let loop ((xs (map car (rootlet))) (out '()))
    (if (null? xs)
        (reverse out)
        (loop (cdr xs)
              (if (denied-authority-name? (car xs))
                  out
                  (cons (car xs) out))))))

(define (capture thunk)
  (catch #t thunk (lambda args (list 'error (car args) args))))

(define (ok? result)
  (not (and (pair? result) (eq? (car result) 'error))))

(define (count pred xs)
  (let loop ((rest xs) (n 0))
    (if (null? rest)
        n
        (loop (cdr rest) (if (pred (car rest)) (+ n 1) n)))))

(define (take xs n)
  (if (or (= n 0) (null? xs))
      '()
      (cons (car xs) (take (cdr xs) (- n 1)))))

(define (summarize-help sym)
  (let* ((h (capture (lambda () (help sym))))
         (d (capture (lambda () (documentation sym))))
         (v (capture (lambda () ((rootlet) sym))))
         (sig (if (ok? v)
                  (capture (lambda () (signature v)))
                  v)))
    (list sym
          (list 'help-ok? (ok? h))
          (list 'help-string? (and (ok? h) (string? h)))
          (list 'help-length (if (and (ok? h) (string? h)) (length h) #f))
          (list 'documentation-ok? (ok? d))
          (list 'signature-ok? (ok? sig)))))

(define summaries (map summarize-help rootlet-symbols))

(list
  (list 'binding-count (length rootlet-symbols))
  (list 'help-ok-count (count (lambda (entry) (cadr (assoc 'help-ok? (cdr entry)))) summaries))
  (list 'help-string-count (count (lambda (entry) (cadr (assoc 'help-string? (cdr entry)))) summaries))
  (list 'documentation-ok-count (count (lambda (entry) (cadr (assoc 'documentation-ok? (cdr entry)))) summaries))
  (list 'signature-ok-count (count (lambda (entry) (cadr (assoc 'signature-ok? (cdr entry)))) summaries))
  (list 'selected (map summarize-help '(car eval lambda* rootlet object->let open-input-string sync-eval))))
