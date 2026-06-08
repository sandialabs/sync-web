;; Imported from upstream s7test.scm line 16654.
;; Original form:
;; (test (catch #t (lambda () (let ((h (hash-table 'b 1))) (set! (h 'b 'asdf) 32))) (lambda (typ info) (apply format #f info)))
;;       "in (set! (h 'b 'asdf) 32), ((hash-table 'b 1) 'b) is 1 which can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((h (hash-table 'b 1))) (set! (h 'b 'asdf) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (h 'b 'asdf) 32), ((hash-table 'b 1) 'b) is 1 which can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16654 actual expected ok?))
