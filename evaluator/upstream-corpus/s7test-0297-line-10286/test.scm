;; Imported from upstream s7test.scm line 10286.
;; Original form:
;; (test (make-list 2) '(#f #f))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 2))))
       (expected (upstream-safe (lambda () '(#f #f))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10286 actual expected ok?))
