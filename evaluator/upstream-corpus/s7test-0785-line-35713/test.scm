;; Imported from upstream s7test.scm line 35713.
;; Original form:
;; (test (let () (define (arg2 a) (let ((b 1)) (set! b (+ a b)) (values b))) (define (hi c) (expt (abs c) (arg2 2))) (hi 2)) 8)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (arg2 a) (let ((b 1)) (set! b (+ a b)) (values b))) (define (hi c) (expt (abs c) (arg2 2))) (hi 2)))))
       (expected (upstream-safe (lambda () 8)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35713 actual expected ok?))
