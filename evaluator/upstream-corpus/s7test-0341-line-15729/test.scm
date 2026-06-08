;; Imported from upstream s7test.scm line 15729.
;; Original form:
;; (test (let ((x '(car (list 1 2 3))))
;; 	(set! (x 0) x)
;; 	(eval (list-values 'let () (list-values 'define '(f1) x) '(catch #t f1 (lambda a 'error)))))
;;       'error)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x '(car (list 1 2 3))))
	(set! (x 0) x)
	(eval (list-values 'let () (list-values 'define '(f1) x) '(catch #t f1 (lambda a 'error))))))))
       (expected (upstream-safe (lambda () 'error)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15729 actual expected ok?))
