;; Imported from upstream s7test.scm line 35645.
;; Original form:
;; (test (let () (define (f) (+ 5 (begin (values 1 2 3) 4))) (f)) 9)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (f) (+ 5 (begin (values 1 2 3) 4))) (f)))))
       (expected (upstream-safe (lambda () 9)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35645 actual expected ok?))
