;; Imported from upstream s7test.scm line 31252.
;; Original form:
;; (test (+ (and (null? ()) 3) (and (zero? 0) 2)) 5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (and (null? ()) 3) (and (zero? 0) 2)))))
       (expected (upstream-safe (lambda () 5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31252 actual expected ok?))
