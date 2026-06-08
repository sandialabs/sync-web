;; Imported from upstream s7test.scm line 16609.
;; Original form:
;; (test (catch #t (lambda () (let ((h (hash-table 'b 1))) (h 'a 'asdf))) (lambda (typ info) (apply format #f info)))
;;       "((hash-table 'b 1) 'a 'asdf) becomes (#f 'asdf), but #f can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((h (hash-table 'b 1))) (h 'a 'asdf))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "((hash-table 'b 1) 'a 'asdf) becomes (#f 'asdf), but #f can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16609 actual expected ok?))
