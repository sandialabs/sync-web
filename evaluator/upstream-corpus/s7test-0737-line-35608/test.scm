;; Imported from upstream s7test.scm line 35608.
;; Original form:
;; (test (+ (do ((i 0 (+ i 1))) ((= i 3) (values i (+ i 1))))) 7)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (do ((i 0 (+ i 1))) ((= i 3) (values i (+ i 1))))))))
       (expected (upstream-safe (lambda () 7)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35608 actual expected ok?))
