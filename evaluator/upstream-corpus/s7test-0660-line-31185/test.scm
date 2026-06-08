;; Imported from upstream s7test.scm line 31185.
;; Original form:
;; (test (+ (or #f (not (null? ())) 3) (or (zero? 1) 2)) 5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (or #f (not (null? ())) 3) (or (zero? 1) 2)))))
       (expected (upstream-safe (lambda () 5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31185 actual expected ok?))
