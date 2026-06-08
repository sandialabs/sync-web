;; Imported from upstream s7test.scm line 21858.
;; Original form:
;; (test (format #f "~{~s ~}" '(fred jerry jill)) "fred jerry jill ")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{~s ~}" '(fred jerry jill)))))
       (expected (upstream-safe (lambda () "fred jerry jill ")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21858 actual expected ok?))
