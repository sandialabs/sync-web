;; Imported from upstream s7test.scm line 16657.
;; Original form:
;; (test (catch #t (lambda () (let ((h (hash-table 'a (hash-table 'b 1)))) (set! (h 'a 'c 'd) 32))) (lambda (typ info) (apply format #f info)))
;;       "in (set! (h 'a 'c 'd) 32), 'c does not exist in (hash-table 'b 1)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((h (hash-table 'a (hash-table 'b 1)))) (set! (h 'a 'c 'd) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (h 'a 'c 'd) 32), 'c does not exist in (hash-table 'b 1)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16657 actual expected ok?))
