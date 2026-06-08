;; Imported from upstream s7test.scm line 30884.
;; Original form:
;; (test (let ((x 0)) (define (func) (catch #t (lambda () (let-temporarily ((x (set! => (+ 1 2)))) x)) (lambda (type info) 'error))) (func) (func)) 'error)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 0)) (define (func) (catch #t (lambda () (let-temporarily ((x (set! => (+ 1 2)))) x)) (lambda (type info) 'error))) (func) (func)))))
       (expected (upstream-safe (lambda () 'error)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30884 actual expected ok?))
