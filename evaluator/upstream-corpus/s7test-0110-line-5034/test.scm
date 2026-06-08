;; Imported from upstream s7test.scm line 5034.
;; Original form:
;; (test (let () (define (func) (catch #t (lambda () (when (not abs cond) #f)) (lambda args 'err))) (define (hi) (func) (func)) (hi) (hi)) 'err)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (func) (catch #t (lambda () (when (not abs cond) #f)) (lambda args 'err))) (define (hi) (func) (func)) (hi) (hi)))))
       (expected (upstream-safe (lambda () 'err)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5034 actual expected ok?))
