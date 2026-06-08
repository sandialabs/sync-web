;; Imported from upstream s7test.scm line 30972.
;; Original form:
;; (test (procedure? (let ((x 0)) (set! x (define (x) 31)) x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? (let ((x 0)) (set! x (define (x) 31)) x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30972 actual expected ok?))
