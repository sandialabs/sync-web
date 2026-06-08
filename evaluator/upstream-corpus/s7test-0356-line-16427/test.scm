;; Imported from upstream s7test.scm line 16427.
;; Original form:
;; (test (let () (define (func) (do ((i 0 (+ i 1))) ((= i 1)) (#_provide :readable))) (func)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (func) (do ((i 0 (+ i 1))) ((= i 1)) (#_provide :readable))) (func)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16427 actual expected ok?))
