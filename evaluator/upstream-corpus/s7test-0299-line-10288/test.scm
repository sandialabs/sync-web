;; Imported from upstream s7test.scm line 10288.
;; Original form:
;; (test (make-list 2/1 1) '(1 1))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 2/1 1))))
       (expected (upstream-safe (lambda () '(1 1))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10288 actual expected ok?))
