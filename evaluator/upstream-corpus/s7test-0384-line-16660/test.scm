;; Imported from upstream s7test.scm line 16660.
;; Original form:
;; (test (catch #t (lambda () (let ((v (hash-table 'a (list 1 2)))) (set! (v 'a 1) 5))) (lambda (typ info) (apply format #f info)))
;;       5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((v (hash-table 'a (list 1 2)))) (set! (v 'a 1) 5))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () 5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16660 actual expected ok?))
