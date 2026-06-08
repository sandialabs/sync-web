;; Exercise object->let over every rootlet symbol and current value.
;; This is intentionally compact: it verifies that introspection can be called
;; broadly without dumping every returned environment.

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

(define (safe-let-ref env key)
  (capture (lambda () (env key))))

(define (summarize-object-let sym)
  (let* ((symbol-info (capture (lambda () (object->let sym))))
         (value-result (capture (lambda () ((rootlet) sym))))
         (value-info (if (ok? value-result)
                         (capture (lambda () (object->let value-result)))
                         value-result)))
    (list sym
          (list 'symbol-info-ok? (and (ok? symbol-info) (let? symbol-info)))
          (list 'symbol-type (if (and (ok? symbol-info) (let? symbol-info))
                                 (safe-let-ref symbol-info 'type)
                                 #f))
          (list 'symbol-doc? (and (ok? symbol-info)
                                  (let? symbol-info)
                                  (string? (safe-let-ref symbol-info '+documentation+))))
          (list 'value-info-ok? (and (ok? value-info) (let? value-info)))
          (list 'value-type (if (and (ok? value-info) (let? value-info))
                                (safe-let-ref value-info 'type)
                                #f))
          (list 'value-arity (if (and (ok? value-info) (let? value-info))
                                 (safe-let-ref value-info 'arity)
                                 #f)))))

(define summaries (map summarize-object-let rootlet-symbols))

(list
  (list 'binding-count (length rootlet-symbols))
  (list 'symbol-object-let-ok-count (count (lambda (entry) (cadr (assoc 'symbol-info-ok? (cdr entry)))) summaries))
  (list 'symbol-doc-count (count (lambda (entry) (cadr (assoc 'symbol-doc? (cdr entry)))) summaries))
  (list 'value-object-let-ok-count (count (lambda (entry) (cadr (assoc 'value-info-ok? (cdr entry)))) summaries))
  (list 'first-ten (take summaries 10))
  (list 'selected (map summarize-object-let '(car eval lambda* rootlet object->let open-input-string sync-eval))))
