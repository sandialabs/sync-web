;; Imported from upstream s7test.scm line 10283.
;; Original form:
;; (test (make-list 1) '(#f))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 1))))
       (expected (upstream-safe (lambda () '(#f))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10283 actual expected ok?))
