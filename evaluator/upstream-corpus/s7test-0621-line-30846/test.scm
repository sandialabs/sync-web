;; Imported from upstream s7test.scm line 30846.
;; Original form:
;; (test (let ((hi 0)) ((set! hi ('((1 2) (3 4)) 0)) 0)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((hi 0)) ((set! hi ('((1 2) (3 4)) 0)) 0)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30846 actual expected ok?))
