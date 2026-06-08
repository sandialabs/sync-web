;; Imported from upstream s7test.scm line 35641.
;; Original form:
;; (test (let () (define (fv) (let ((x (list-values (values)))) (null? x))) (fv)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (fv) (let ((x (list-values (values)))) (null? x))) (fv)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35641 actual expected ok?))
