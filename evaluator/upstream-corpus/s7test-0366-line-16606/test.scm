;; Imported from upstream s7test.scm line 16606.
;; Original form:
;; (test (catch #t (lambda () (hash-table-ref (hash-table 'a 1) 'b 2)) (lambda (typ info) (apply format #f info)))
;;       "(hash-table-ref (hash-table 'a 1) 'b 2) becomes (#f 2), but #f can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (hash-table-ref (hash-table 'a 1) 'b 2)) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "(hash-table-ref (hash-table 'a 1) 'b 2) becomes (#f 2), but #f can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16606 actual expected ok?))
