;; Imported from upstream s7test.scm line 31248.
;; Original form:
;; (test (let ((a 1)) (and (let () (set! a 2) #t) (= a 1) (let () (set! a 3) #f) (and (= a 3) a) (let () (set! a 4) #f) a)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (and (let () (set! a 2) #t) (= a 1) (let () (set! a 3) #f) (and (= a 3) a) (let () (set! a 4) #f) a)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31248 actual expected ok?))
