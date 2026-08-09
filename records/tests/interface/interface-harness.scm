(lambda (scenario-src root-src standard-src chain-src tree-src ledger-src federation-src authorization-src interface-src)

  (define submitted-actions '())

  (define factory
  (lambda* (root-src
            standard-src
            chain-src
            tree-src
            ledger-src
            federation-src
            authorization-src
            interface-src
            (journals 1)
            (journal-start 1)
            (users '(alice))
            (admins '())
            (window 4)
            (tick 0))

    (define native-scenario-submit scenario-submit)
    (define native-scenario-await scenario-await)
    (define configured-users
      (begin
        (for-each
          (lambda (user)
            (if (not (symbol? user))
                (error 'argument-error "User names must be symbols: ~S" user)))
          users)
        users))
    (define undefined ((inlet) 'undefined))

    (define (journal-url number)
      (append "http://journal-" (number->string number) ".test/interface"))

    (define (journal-name number)
      (string->symbol (append "journal-" (number->string number))))

    (define (root-secret number)
      (append "pass-" (number->string number)))

    (define (make-journal number)
      (let ((name (journal-name number))
            (url (journal-url number))
            (credentials (journal-url number))
            (root-credential (root-secret number)))
        (lambda (field . value)
          (case field
            ((kind) 'journal)
            ((number) number)
            ((name) name)
            ((url) url)
            ((credentials)
             (if (null? value) credentials
                 (begin (set! credentials (car value)) credentials)))
            ((root-secret)
             (if (null? value) root-credential
                 (begin (set! root-credential (car value)) root-credential)))
            (else (error 'journal-error "Unknown journal field: ~S" field))))))

    (define journal-list
      (let loop ((number journal-start) (result '()))
        (if (>= number (+ journal-start journals))
            (reverse result)
            (loop (+ number 1) (cons (make-journal number) result)))))

    (define (journal-ref number)
      (let loop ((remaining journal-list))
        (cond ((null? remaining)
               (error 'journal-error "Unknown numeric journal: ~S" number))
              ((= ((car remaining) 'number) number) (car remaining))
              (else (loop (cdr remaining))))))

    (define (journal? value)
      (and (procedure? value)
           (catch #t
                  (lambda () (eq? (value 'kind) 'journal))
                  (lambda args #f))))

    (define (interface-config journal clear? admins window)
      `((clear? ,clear?)
        (root-secret ,(journal 'root-secret))
        (interface-secret ,(journal 'url))
        (admins ,admins)
        (window ,window)
        (root ,root-src)
        (interface ,(journal 'url))
        (name ,(journal 'url))))

    (define (install-expression journal)
      `(,interface-src ,(interface-config journal #t admins window)
                       ',standard-src ',chain-src ',tree-src ',ledger-src
                       ',federation-src ',authorization-src))

    ;; Interface installation is ordinary scheduled work, submitted before
    ;; scenario actions for each generated environment.
    (define setup-descriptors
      (map (lambda (journal)
             (list (journal 'url) (install-expression journal)))
           journal-list))

    (define (split-keywords args)
      (let loop ((remaining args) (positionals '()) (keywords '()))
        (cond ((null? remaining) (list (reverse positionals) (reverse keywords)))
              ((keyword? (car remaining))
               (if (null? (cdr remaining))
                   (error 'argument-error "Missing value for keyword: ~S" (car remaining)))
               (loop (cddr remaining)
                     positionals
                     (cons (list (keyword->symbol (car remaining)) (cadr remaining)) keywords)))
              (else
               (loop (cdr remaining) (cons (car remaining) positionals) keywords)))))

    (define (keyword-ref keywords key default)
      (let ((entry (assoc key keywords)))
        (if entry (cadr entry) default)))

    (define (normalize-schedule schedule)
      (cond ((list? schedule) schedule)
            ((vector? schedule) (vector->list schedule))
            (else
             (error 'argument-error
                    "Schedule must be a list or vector: ~S" schedule))))

    (define (api-arguments function args)
      (let* ((split (split-keywords args))
             (positionals (car split))
             (keywords (cadr split)))
        (case function
          ((set!)
           (let ((arguments
                  (cond ((>= (length positionals) 2)
                         (append `((path ,(car positionals))
                                   (value ,(cadr positionals))) keywords))
                        ((= (length positionals) 1)
                         (cons `(path ,(car positionals)) keywords))
                        ((assoc 'path keywords) keywords)
                        (else (error 'argument-error "set! requires a path")))))
             (if (or (assoc 'expression? arguments)
                     (not (assoc 'value arguments)))
                 arguments
                 (cons '(expression? #t) arguments))))
          ((get resolve)
           (let ((arguments
                  (cond ((pair? positionals)
                         (cons `(path ,(car positionals)) keywords))
                        ((assoc 'path keywords) keywords)
                        (else (error 'argument-error "~S requires a path" function)))))
             (if (assoc 'expression? arguments)
                 arguments
                 (cons '(expression? #t) arguments))))
          ((get-batch resolve-batch pin-batch! unpin-batch!)
           (let ((arguments
                  (cond ((pair? positionals)
                         (cons `(paths ,(car positionals)) keywords))
                        ((assoc 'paths keywords) keywords)
                        (else (error 'argument-error "~S requires paths" function)))))
             (if (and (memq function '(get-batch resolve-batch))
                      (not (assoc 'expression? arguments)))
                 (cons '(expression? #t) arguments)
                 arguments)))
          ((call!)
           (cond ((>= (length positionals) 2)
                  (append `((path ,(car positionals))
                            (arguments ,(cadr positionals))) keywords))
                 ((and (assoc 'path keywords) (assoc 'arguments keywords)) keywords)
                 (else
                  (error 'argument-error "call! requires a path and arguments list"))))
          ((pin! unpin! delete-bridge!)
           (if (null? positionals)
               (error 'argument-error "~S requires one argument" function))
           (cons (list (if (eq? function 'delete-bridge!) 'name 'path)
                       (car positionals))
                 keywords))
          (else
           (cond ((null? positionals) keywords)
                 ((and (= (length positionals) 1) (list? (car positionals)))
                  (append (car positionals) keywords))
                 (else
                  (error 'argument-error
                         "Use named arguments for ~S: ~S"
                         function args)))))))

    (define (query principal origin route function api-args history)
      (let ((root? (eq? principal '*journal*))
            (anonymous? (eq? principal '*anonymous*)))
        (if (and root? (eq? function 'step!))
            `(*step* ,(origin 'root-secret))
            (let* ((committed? (and (pair? route) (memq function '(resolve pin! unpin!))))
                   (indexes
                    (and committed?
                         (if (not (undefined? history)) history
                             (let* ((defaults (make-list (+ (length route) 1) -1))
                                    (path (cadr (assoc 'path
                                                       (api-arguments function api-args))))
                                    (terminal (and (pair? path) (integer? (car path))
                                                   (car path))))
                               (if (not terminal) defaults
                                   (append (reverse (cdr (reverse defaults)))
                                           (list terminal)))))))
                   (arguments
                    (if (eq? function 'bridge!)
                        (let ((remote (car api-args)))
                          (if (not (journal? remote))
                              (error 'argument-error "bridge! requires a journal: ~S" remote))
                          `((name ,(remote 'name))
                            (interface ,(remote 'url))
                            (remote-name ,(origin 'name))))
                        (api-arguments function api-args)))
                   (arguments
                    (if (not committed?) arguments
                        (let* ((path (cadr (assoc 'path arguments)))
                               (tail (if (and (pair? path) (integer? (car path)))
                                         (cdr path) path))
                               (full
                                (let loop ((journals route) (rest (cdr indexes))
                                           (out (list (car indexes))))
                                  (if (null? journals) (append out tail)
                                      (loop (cdr journals) (cdr rest)
                                            (append out
                                                    (list ((car journals) 'name)
                                                          (car rest))))))))
                          (map (lambda (entry)
                                 (if (eq? (car entry) 'path) (list 'path full) entry))
                               arguments))))
                   (request
                    `((function ,function)
                      ,@(if (null? arguments) '() `((arguments ,arguments))))))
              (cond ((and (pair? route) (not committed?))
                     (if root?
                         (error 'argument-error "Root journal operations cannot use a federation route"))
                     (append
                      request
                      `((invocation
                         ((identity ,principal)
                          (route-source ())
                          (route-target ,(map (lambda (journal) (journal 'name)) route))
                          (credentials ,(origin 'credentials)))))))
                    (root?
                     (append request
                             `((authentication ((credentials ,(origin 'credentials)))))))
                    (anonymous? request)
                    (else
                     (append request
                             `((authentication
                                ((identity (*state* ,principal))
                                 (credentials ,(origin 'credentials))))))))))))

    (define (make-action descriptor label)
      (let ((result undefined)
            (state 'pending)
            (expectation undefined)
            (schedule '())
            (tick 0))
        (lambda args
          (if (null? args)
              result
              (case (car args)
                ((~descriptor) descriptor)
                ((~expectation) expectation)
                ((~schedule) schedule)
                ((~tick) tick)
                ((~label) label)
                ((~submit!)
                 (if (not (eq? state 'pending))
                     (error 'scenario-error "Action has already been submitted: ~S" label))
                 (set! expectation (cadr args))
                 (set! schedule (caddr args))
                 (set! tick (cadddr args))
                 (set! state 'submitted)
                 #t)
                ((~complete!)
                 (set! result (cadr args))
                 (set! state 'complete)
                 result)
                (else
                 (error 'scenario-error
                        "Unknown action operation: ~S" (car args))))))))

    (define (make-principal principal)
      (lambda context
        (let* ((split (split-keywords context))
               (positionals (car split))
               (metadata (cadr split))
               (history (keyword-ref metadata 'history undefined)))
          (if (not (= (length metadata)
                      (if (undefined? history) 0 1)))
              (error 'argument-error
                     "Unknown action metadata; use test-submit: ~S" metadata))
          (if (< (length positionals) 2)
              (error 'argument-error
                     "Principal dispatch requires an origin journal and function"))
          (let* ((function (car (reverse positionals)))
                 (journals (reverse (cdr (reverse positionals))))
                 (origin (car journals))
                 (route (cdr journals)))
            (if (not (and (symbol? function)
                          (not (memq #f (map journal? journals)))))
                (error 'argument-error "Invalid principal dispatch: ~S" context))
            (lambda api-args
              (make-action
                (list (origin 'url)
                      (query principal origin route function api-args history))
                (list principal
                      (map (lambda (journal) (journal 'name)) journals)
                      function
                      api-args)))))))

    (define (test-submit action . metadata)
      (if (not (procedure? action))
          (error 'scenario-error "test-submit expects an action thunk: ~S" action))
      (let* ((split (split-keywords metadata))
             (positionals (car split))
             (keywords (cadr split)))
        (if (pair? positionals)
            (error 'argument-error
                   "test-submit accepts only keyword metadata: ~S" positionals))
        (let ((expectation
               (keyword-ref keywords 'expect (action '~expectation)))
              (schedule
               (normalize-schedule
                 (keyword-ref keywords 'schedule (action '~schedule))))
              (action-tick (keyword-ref keywords 'tick tick)))
          (action '~submit! expectation schedule action-tick)
          (native-scenario-submit
            (append (action '~descriptor) (list schedule action-tick)))
          (set! submitted-actions (append submitted-actions (list action)))
          action)))

    (define (test-await)
      (if (null? submitted-actions)
          (error 'scenario-error "test-await has no submitted action"))
      (let* ((action (car submitted-actions))
             (result (native-scenario-await))
             (expected (action '~expectation)))
        (set! submitted-actions (cdr submitted-actions))
        (action '~complete! result)
        (if (not (undefined? expected))
            (let* ((render
                    (lambda (value)
                      (catch #t
                             (lambda () (object->string value))
                             (lambda args "<unprintable>"))))
                   (truncate
                    (lambda (value)
                      (if (< (length value) 256)
                          value
                          (append (substring value 0 256) " ..."))))
                   (expectation-error #f)
                   (matched
                    (catch #t
                           (lambda ()
                             (if (procedure? expected)
                                 (expected result)
                                 (equal? expected result)))
                           (lambda args
                             (set! expectation-error args)
                             #f))))
              (if matched
                  (set! checks (+ checks 1))
                  (error
                    'expectation-failure
                    (append
                      "[Check " (object->string (+ checks 1)) " failed] "
                      "[Action " (render (action '~label)) "] "
                      "[Result " (truncate (render result)) "] "
                      "[Expected " (truncate (render expected)) "]"
                      (if expectation-error
                          (append " [Expectation error "
                                  (truncate (render expectation-error)) "]")
                          "")
                      "\n[Stacktrace\n" (stacktrace 20 120 180 120 #f) "]")))))
        result))

    (define (test-report)
      (let loop ()
        (if (pair? submitted-actions)
            (begin (test-await) (loop))
            (append "Success (" (object->string checks) " checks)"))))

    (for-each
      (lambda (descriptor)
        (test-submit
          (make-action descriptor (list '*journal* (car descriptor) 'install))
          :expect "Installed interface"))
      setup-descriptors)

    (define (raw journal expression)
      (make-action
        (list (journal 'url) expression)
        (list 'raw (journal 'name) expression)))

    (define* (update-interface journal (admins '()) (window 4) (clear? #f))
      (make-action
        (list
          (journal 'url)
          `(*eval* ,(journal 'root-secret)
                   (,interface-src ,(interface-config journal clear? admins window)
                                   ',standard-src ',chain-src ',tree-src ',ledger-src
                                   ',federation-src ',authorization-src)))
        (list '*journal* (list (journal 'name)) 'update-interface
              admins window clear?)))

    ;; Keep the harness helpers in the outlet and add only generated public names
    ;; to a child environment. `varlet` can resolve values through that outlet.
    (define environment (sublet (curlet)))
    (varlet environment '*journal* (make-principal '*journal*))
    (varlet environment '*anonymous* (make-principal '*anonymous*))
    (for-each
      (lambda (journal)
        (varlet environment (journal 'name) journal))
      journal-list)
    (for-each
      (lambda (user)
        (varlet environment user (make-principal user)))
      configured-users)
    environment))

  (define* (make-interface-harness
            (journals 1)
            (journal-start 1)
            (users '(alice))
            (admins '())
            (window 4)
            (tick 0))
    (factory root-src standard-src chain-src tree-src ledger-src federation-src
             authorization-src interface-src :journals journals
             :journal-start journal-start :users users :admins admins
             :window window :tick tick))

  (define checks 0)

  ((eval scenario-src) make-interface-harness))
