;; Imported from upstream s7test.scm line 30951.
;; Original form:
;; (test (let () (define (func) (let ((x (make-int-vector '(2 3) 0))) (set! (x 0 0) 1) (x 0 0))) (define (hi) (func)) (hi) (hi)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (func) (let ((x (make-int-vector '(2 3) 0))) (set! (x 0 0) 1) (x 0 0))) (define (hi) (func)) (hi) (hi)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30951 actual expected ok?))
