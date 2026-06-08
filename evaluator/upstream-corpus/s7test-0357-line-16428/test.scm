;; Imported from upstream s7test.scm line 16428.
;; Original form:
;; (test (pair? (let ()
;;                (define (func)
;;                  (list-values (#_quasiquote (odd?)) (let ((<1> (list 1 #f))) (set! (<1> 1) (let ((<L> (list #f 3))) (set-car! <L> <1>) <L>)) <1>)))
;;                (func)))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (pair? (let ()
               (define (func)
                 (list-values (#_quasiquote (odd?)) (let ((<1> (list 1 #f))) (set! (<1> 1) (let ((<L> (list #f 3))) (set-car! <L> <1>) <L>)) <1>)))
               (func))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16428 actual expected ok?))
