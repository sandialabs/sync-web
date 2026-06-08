;; Imported from upstream s7test.scm line 31005.
;; Original form:
;; (test (apply (inlet) '(define y (catch #t (lambda () (+ 1 #\a)) (lambda (type info) 32)))) 32)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply (inlet) '(define y (catch #t (lambda () (+ 1 #\a)) (lambda (type info) 32)))))))
       (expected (upstream-safe (lambda () 32)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31005 actual expected ok?))
