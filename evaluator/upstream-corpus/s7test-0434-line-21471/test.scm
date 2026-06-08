;; Imported from upstream s7test.scm line 21471.
;; Original form:
;; (test (let (({ 3)) (+ { 1)) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let (({ 3)) (+ { 1)))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21471 actual expected ok?))
