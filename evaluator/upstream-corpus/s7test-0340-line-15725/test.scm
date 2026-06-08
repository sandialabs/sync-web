;; Imported from upstream s7test.scm line 15725.
;; Original form:
;; (test (let ((x '(1 2 3)))
;; 	(set! (x 0) (cons x 2))
;; 	(eval (list-values 'let () (list-values 'define '(f1) (list-values 'list-set! x 0 (list-values 'cons x 2))) '(catch #t f1 (lambda a 'error)))))
;;       'error)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(1 2 3)))
	(set! (x 0) (cons x 2))
	(eval (list-values 'let () (list-values 'define '(f1) (list-values 'list-set! x 0 (list-values 'cons x 2))) '(catch #t f1 (lambda a 'error))))))))
       (expected (upstream-safe (lambda () 'error)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15725 actual expected ok?))
