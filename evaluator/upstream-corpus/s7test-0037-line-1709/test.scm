;; Imported from upstream s7test.scm line 1709.
;; Original form:
;; (test (eq? (string #\h #\i) (string #\h #\i)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (string #\h #\i) (string #\h #\i)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1709 actual expected ok?))
