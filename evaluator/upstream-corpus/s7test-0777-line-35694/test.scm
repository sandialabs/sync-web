;; Imported from upstream s7test.scm line 35694.
;; Original form:
;; (test (signature (hash-table)) (let ((sig (list #t 'hash-table? #t))) (set-cdr! (cddr sig) (cddr sig)) sig))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (signature (hash-table)))))
       (expected (upstream-safe (lambda () (let ((sig (list #t 'hash-table? #t))) (set-cdr! (cddr sig) (cddr sig)) sig))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35694 actual expected ok?))
