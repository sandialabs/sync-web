;; Exercise help/documentation/signature over every rootlet binding.
;; Return compact counts rather than the full help text corpus.

(define rootlet-symbols (map car (rootlet)))

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
  (list 'first-ten (take summaries 10))
  (list 'selected (map summarize-help '(car eval lambda* rootlet object->let open-input-string sync-eval))))
