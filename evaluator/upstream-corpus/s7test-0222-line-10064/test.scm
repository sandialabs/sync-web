;; Imported from upstream s7test.scm line 10064.
;; Original form:
;; (test (apply list-set! '((1 2) (3 2)) 1 '(1 2)) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply list-set! '((1 2) (3 2)) 1 '(1 2)))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10064 actual expected ok?))
