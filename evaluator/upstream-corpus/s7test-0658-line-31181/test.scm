;; Imported from upstream s7test.scm line 31181.
;; Original form:
;; (test (let ((a 1)) (or (let () (set! a 2) #f) (= a 1) (let () (set! a 3) #f) (and (= a 3) a) (let () (set! a 4) #f) a)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (or (let () (set! a 2) #f) (= a 1) (let () (set! a 3) #f) (and (= a 3) a) (let () (set! a 4) #f) a)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31181 actual expected ok?))
