;; A compact metacircular evaluator for a small Scheme subset.
;; It intentionally uses only sync-web-profile-safe host features.

(define (tagged-list? exp tag)
  (and (pair? exp) (eq? (car exp) tag)))

(define (self-evaluating? exp)
  (or (number? exp) (string? exp) (boolean? exp) (char? exp) (byte-vector? exp)))

(define (variable? exp) (symbol? exp))
(define (quoted? exp) (tagged-list? exp 'quote))
(define (text-of-quotation exp) (cadr exp))
(define (assignment? exp) (tagged-list? exp 'set!))
(define (assignment-variable exp) (cadr exp))
(define (assignment-value exp) (caddr exp))
(define (definition? exp) (tagged-list? exp 'define))

(define (definition-variable exp)
  (if (symbol? (cadr exp))
      (cadr exp)
      (caadr exp)))

(define (definition-value exp)
  (if (symbol? (cadr exp))
      (caddr exp)
      (cons 'lambda (cons (cdadr exp) (cddr exp)))))

(define (if? exp) (tagged-list? exp 'if))
(define (if-predicate exp) (cadr exp))
(define (if-consequent exp) (caddr exp))
(define (if-alternative exp) (if (null? (cdddr exp)) #f (cadddr exp)))
(define (lambda? exp) (tagged-list? exp 'lambda))
(define (lambda-parameters exp) (cadr exp))
(define (lambda-body exp) (cddr exp))
(define (begin? exp) (tagged-list? exp 'begin))
(define (begin-actions exp) (cdr exp))
(define (application? exp) (pair? exp))
(define (operator exp) (car exp))
(define (operands exp) (cdr exp))

(define (true? x) (not (eq? x #f)))

(define (make-frame variables values)
  (if (null? variables)
      '()
      (cons (cons (car variables) (car values))
            (make-frame (cdr variables) (cdr values)))))

(define (extend-environment variables values base-env)
  (if (= (length variables) (length values))
      (cons (make-frame variables values) base-env)
      (error 'mc-arity-error "wrong number of arguments: ~S vs ~S" variables values)))

(define (lookup-variable-value var env)
  (let env-loop ((env env))
    (if (null? env)
        (error 'mc-unbound-variable "unbound variable: ~S" var)
        (let ((binding (assoc var (car env))))
          (if binding
              (cdr binding)
              (env-loop (cdr env)))))))

(define (set-variable-value! var val env)
  (let env-loop ((env env))
    (if (null? env)
        (error 'mc-unbound-variable "unbound variable in set!: ~S" var)
        (let ((binding (assoc var (car env))))
          (if binding
              (begin (set-cdr! binding val) 'ok)
              (env-loop (cdr env)))))))

(define (define-variable! var val env)
  (let* ((frame (car env))
         (binding (assoc var frame)))
    (if binding
        (set-cdr! binding val)
        (set-car! env (cons (cons var val) frame)))
    'ok))

(define (make-primitive proc) (list 'primitive proc))
(define (primitive-procedure? proc) (tagged-list? proc 'primitive))
(define (primitive-implementation proc) (cadr proc))
(define (apply-primitive-procedure proc args)
  (apply (primitive-implementation proc) args))

(define primitive-procedures
  (list
    (list '+ +) (list '- -) (list '* *) (list '/ /)
    (list '= =) (list '< <) (list '<= <=) (list '> >) (list '>= >=)
    (list 'cons cons) (list 'car car) (list 'cdr cdr) (list 'list list)
    (list 'null? null?) (list 'pair? pair?) (list 'not not)
    (list 'eq? eq?) (list 'equal? equal?)
    (list 'length length) (list 'append append) (list 'reverse reverse)))

(define (setup-environment)
  (list
    (map (lambda (entry)
           (cons (car entry) (make-primitive (cadr entry))))
         primitive-procedures)))

(define (make-procedure parameters body env)
  (list 'procedure parameters body env))

(define (compound-procedure? proc) (tagged-list? proc 'procedure))
(define (procedure-parameters proc) (cadr proc))
(define (procedure-body proc) (caddr proc))
(define (procedure-environment proc) (cadddr proc))

(define (list-of-values exps env)
  (if (null? exps)
      '()
      (cons (mc-eval (car exps) env)
            (list-of-values (cdr exps) env))))

(define (eval-sequence exps env)
  (cond ((null? exps) '())
        ((null? (cdr exps)) (mc-eval (car exps) env))
        (else (mc-eval (car exps) env)
              (eval-sequence (cdr exps) env))))

(define (mc-apply procedure arguments)
  (cond ((primitive-procedure? procedure)
         (apply-primitive-procedure procedure arguments))
        ((compound-procedure? procedure)
         (eval-sequence
           (procedure-body procedure)
           (extend-environment
             (procedure-parameters procedure)
             arguments
             (procedure-environment procedure))))
        (else (error 'mc-unknown-procedure "unknown procedure type: ~S" procedure))))

(define (mc-eval exp env)
  (cond ((self-evaluating? exp) exp)
        ((variable? exp) (lookup-variable-value exp env))
        ((quoted? exp) (text-of-quotation exp))
        ((assignment? exp)
         (set-variable-value! (assignment-variable exp)
                              (mc-eval (assignment-value exp) env)
                              env))
        ((definition? exp)
         (define-variable! (definition-variable exp)
                           (mc-eval (definition-value exp) env)
                           env))
        ((if? exp)
         (if (true? (mc-eval (if-predicate exp) env))
             (mc-eval (if-consequent exp) env)
             (mc-eval (if-alternative exp) env)))
        ((lambda? exp)
         (make-procedure (lambda-parameters exp) (lambda-body exp) env))
        ((begin? exp)
         (eval-sequence (begin-actions exp) env))
        ((application? exp)
         (mc-apply (mc-eval (operator exp) env)
                   (list-of-values (operands exp) env)))
        (else (error 'mc-unknown-expression "unknown expression type: ~S" exp))))

(define the-global-environment (setup-environment))

(list
  (mc-eval '(begin
              (define (fact n)
                (if (= n 0)
                    1
                    (* n (fact (- n 1)))))
              (fact 6))
           the-global-environment)
  (mc-eval '(begin
              (define (make-adder x)
                (lambda (y) (+ x y)))
              (define add10 (make-adder 10))
              (add10 7))
           the-global-environment)
  (mc-eval '(begin
              (define counter 0)
              (define (next!)
                (begin
                  (set! counter (+ counter 1))
                  counter))
              (list (next!) (next!) counter))
           the-global-environment)
  (mc-eval '(begin
              (define (fold-left f init xs)
                (if (null? xs)
                    init
                    (fold-left f (f init (car xs)) (cdr xs))))
              (fold-left + 0 (quote (1 2 3 4 5))))
           the-global-environment))
